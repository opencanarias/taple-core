use std::collections::HashMap;

use crate::{
    commons::{
        models::{
            approval::{Approval, ApprovalContent},
            event_proposal::EventProposal,
            state::{Subject},
            Acceptance,
        },
        self_signature_manager::{SelfSignatureInterface, SelfSignatureManager}, config::VotationType,
    },
    database::DB,
    event_content::Metadata,
    event_request::{EventRequest, EventRequestType},
    governance::{error::RequestError, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    DatabaseManager, Notification,
};

use super::{
    error::{ApprovalErrorResponse, ApprovalManagerError}, ApprovalPetitionData,
};
use crate::database::Error as DbError;
use crate::governance::stage::ValidationStage;

pub trait NotifierInterface {
    fn request_reached(&self, id: &str, subject_id: &str);
    fn request_deleted(&self, id: &str, subject_id: &str);
}

pub struct RequestNotifier {
    sender: tokio::sync::broadcast::Sender<Notification>,
}

impl RequestNotifier {
    pub fn new(sender: tokio::sync::broadcast::Sender<Notification>) -> Self {
        Self { sender }
    }
}

impl NotifierInterface for RequestNotifier {
    fn request_reached(&self, id: &str, subject_id: &str) {
        let _ = self.sender.send(Notification::RequestReached {
            request_id: id.clone().to_owned(),
            subject_id: subject_id.clone().to_owned(),
            default_message: format!(
                "Se ha recibido la petición {} del sujeto {}",
                id, subject_id
            ),
        });
    }

    fn request_deleted(&self, id: &str, subject_id: &str) {
        let _ = self.sender.send(Notification::RequestDeleted {
            request_id: id.clone().to_owned(),
            subject_id: subject_id.clone().to_owned(),
            default_message: format!(
                "Se ha borrado la petición {} del sujeto {} debido a un cambio en la gobernanza",
                id, subject_id
            ),
        });
    }
}

pub struct InnerApprovalManager<G: GovernanceInterface, D: DatabaseManager, N: NotifierInterface> {
    governance: G,
    database: DB<D>,
    notifier: N,
    signature_manager: SelfSignatureManager,
    // Cola de 1 elemento por sujeto
    subject_been_approved: HashMap<DigestIdentifier, DigestIdentifier>, // SubjectID -> ReqId
    request_table: HashMap<DigestIdentifier, ApprovalPetitionData>, // RequestID -> (SubjectID, SN, GovID, GovVersion)
    pass_votation: VotationType,
}

impl<G: GovernanceInterface, D: DatabaseManager, N: NotifierInterface>
    InnerApprovalManager<G, D, N>
{
    pub fn new(
        governance: G,
        database: DB<D>,
        notifier: N,
        signature_manager: SelfSignatureManager,
        pass_votation: VotationType,
    ) -> Self {
        Self {
            governance,
            database,
            notifier,
            signature_manager,
            subject_been_approved: HashMap::new(),
            request_table: HashMap::new(),
            pass_votation,
        }
    }

    pub fn get_single_request(&self, request_id: &DigestIdentifier) -> Result<ApprovalPetitionData, ApprovalErrorResponse> {
        let Some(request) = self.request_table.get(request_id) else {
            return Err(ApprovalErrorResponse::ApprovalRequestNotFound);
        };
        Ok(request.clone())
    }

    pub fn get_all_request(&self) -> Vec<ApprovalPetitionData> {
        self.request_table.values().cloned().collect()
    }

    pub fn change_pass_votation(&mut self, pass_votation: VotationType) {
        self.pass_votation = pass_votation;
    }

    pub async fn get_governance_version(
        &self,
        governance_id: &DigestIdentifier,
    ) -> Result<Result<u64, ApprovalErrorResponse>, ApprovalManagerError> {
        match self.governance.get_governance_version(governance_id.to_owned()).await {
            Ok(data) => Ok(Ok(data)),
            Err(RequestError::GovernanceNotFound(_str)) => {
                Ok(Err(ApprovalErrorResponse::GovernanceNotFound))
            }
            Err(RequestError::InvalidGovernanceID) => {
                Ok(Err(ApprovalErrorResponse::InvalidGovernanceID))
            }
            Err(RequestError::ChannelClosed) => Err(ApprovalManagerError::GovernanceChannelFailed),
            Err(_error) => Err(ApprovalManagerError::UnexpectedError),
        }
    }

    pub fn new_governance_version(
        &mut self,
        governance_id: &DigestIdentifier,
        governance_version: u64,
    ) {
        // Comprobamos todas las peticiones guardadas y borramos las afectadas
        for (req_id, data) in self.request_table.iter() {
            if &data.governance_id == governance_id {
                if governance_version > data.governance_version {
                    // Afectado por el cambio de governance
                    self.subject_been_approved.remove(&data.subject_id);
                    self.subject_been_approved.remove(&req_id);
                    // Notificar por el canal
                    self.notifier
                        .request_deleted(&req_id.to_str(), &data.subject_id.to_str());
                }
            }
        }
    }

    pub async fn process_approval_request(
        &mut self,
        approval_request: EventProposal,
    ) -> Result<
        Result<Option<(Approval, KeyIdentifier)>, ApprovalErrorResponse>,
        ApprovalManagerError,
    > {
        /*
         EL APROBADOR AHORA TAMBIÉN ES TESTIGO
         - Comprobar si se posee el sujeto
         - Comprobar si estamos sincronizados
         Comprobamos la versión de la gobernanza
           - Rechazamos las peticiones que tengan una versión de gobernanza distinta a la nuestra
         Comprobamos la validez criptográfica de la información que nos entrega.
           - Comprobar la firma de invocación.
           - Comprobar validez del invocador.
           - Comprobar las firmas de evaluación.
           - Comprobar la validez de los evaluadores.
         Las peticiones no se van a guardar en la BBDD, pero sí en memoria.
         Solo se guardará una petición por sujeto. Existe la problemática de que un evento haya sido aprobado sin nuestra
         intervención. En ese caso es precisar eliminar la petición y actualizar a la nueva.
         Debemos comprobar siempre si ya tenemos la petición que nos envían.
        */

        let id = &approval_request
            .proposal
            .event_request
            .signature
            .content
            .event_content_hash;

        if let Some(_data) = self.request_table.get(&id) {
            return Ok(Err(ApprovalErrorResponse::RequestAlreadyKnown));
        }

        // Comprobamos si la request es de tipo State
        let EventRequestType::State(state_request) = &approval_request.proposal.event_request.request else {
                return Ok(Err(ApprovalErrorResponse::NoFactEvent));
            };

        // Comprobamos si tenemos el sujeto y si estamos sincronizados
        let subject_data = match self.database.get_subject(&state_request.subject_id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => return Ok(Err(ApprovalErrorResponse::SubjectNotFound)),
            Err(_error) => return Err(ApprovalManagerError::DatabaseError),
        };

        if approval_request.proposal.sn > subject_data.sn + 1 {
            return Ok(Err(ApprovalErrorResponse::SubjectNotSynchronized));
        }

        // Comprobamos si ya estamos aprobando el sujeto para un evento igual o mayor.
        // En caso de no haber request previa, continuamos.
        if let Some(prev_request_id) = self.subject_been_approved.get(&state_request.subject_id) {
            let data = self.request_table.get(&prev_request_id).unwrap();
            if approval_request.proposal.sn <= data.sn {
                return Ok(Err(ApprovalErrorResponse::PreviousEventDetected));
            }
        }

        // // Comprobamos si el ID de la gobernanza del sujeto que tenemos registrado coincide con el especificado
        // if subject_data.governance_id != approval_request.proposal.governance_id {
        //     return Ok(Err(ApprovalErrorResponse::GovernanceNoCorrelation));
        // }

        // Comprobamos si la versión de la gobernanza es correcta
        let version = match self
            .get_governance_version(&subject_data.governance_id)
            .await?
        {
            Ok(version) => version,
            Err(error) => return Ok(Err(error)),
        };

        let evaluation = approval_request.proposal.evaluation.clone().expect("los genesis no se aprueban");

        if version != evaluation.governance_version {
            return Ok(Err(ApprovalErrorResponse::InvalidGovernanceVersion));
        }

        let metadata = create_metadata(&subject_data, version);

        // Comprobar si somos aprobadores. Esto antes incluso que la firma del sujeto
        let approvers_list = self
            .governance
            .get_signers(metadata.clone(), ValidationStage::Approve)
            .await
            .map_err(|_| ApprovalManagerError::GovernanceChannelFailed)?;
        let current_node = self.signature_manager.get_own_identifier();
        if !approvers_list.contains(&current_node) {
            return Ok(Err(ApprovalErrorResponse::NodeIsNotApprover));
        }

        // Comprobamos validez criptográfica de la firma del sujeto
        // Empezamos comprobando que el firmante sea el sujeto
        if approval_request.subject_signature.content.signer != subject_data.public_key {
            return Ok(Err(ApprovalErrorResponse::SignatureSignerIsNotSubject));
        }

        // Verificamos la firma
        let hash = event_proposal_hash_gen(&approval_request)?;
        if let Err(_error) = approval_request.subject_signature.content.signer.verify(
            &hash.derivative(),
            &approval_request.subject_signature.signature,
        ) {
            return Ok(Err(ApprovalErrorResponse::InvalidSubjectSignature));
        }

        // Verificamos que el invocador es váĺido
        ;
        if self
            .check_event_request_signatures(&approval_request.proposal.event_request)?
            .is_err()
        {
            return Ok(Err(ApprovalErrorResponse::InvalidInvokator));
        }

        // if !self
        //     .governance
        //     .has_invokator_permission(
        //         &subject_data.governance_id,
        //         &subject_data.schema_id,
        //         &subject_data.namespace,
        //     )
        //     .await
        //     .map_err(|_| ApprovalManagerError::GovernanceChannelFailed)?
        // {
        //     return Ok(Err(ApprovalErrorResponse::InvalidInvokatorPermission));
        // }

        // Verificamos las evaluaciones
        // Se tiene que verificar tanto las firmas como que los firmantes sean evaluadores válidos para la versión de la gobernanza
        let evaluators = self
            .governance
            .get_signers(metadata.clone(), ValidationStage::Evaluate)
            .await
            .map_err(|_| ApprovalManagerError::GovernanceChannelFailed)?;

        let hash = DigestIdentifier::from_serializable_borsh(&evaluation)
            .map_err(|_| ApprovalManagerError::HashGenerationFailed)?;

        for signature in approval_request.proposal.evaluation_signatures.iter() {
            // Comprobación de que el evaluador existe
            if !evaluators.contains(&signature.content.signer) {
                return Ok(Err(ApprovalErrorResponse::InvalidEvaluator));
            }
            // Comprobamos su firma -> Es necesario generar el contenido que ellos firman
            if signature
                .content
                .signer
                .verify(&hash.derivative(), &signature.signature)
                .is_err()
            {
                return Ok(Err(ApprovalErrorResponse::InvalidEvaluatorSignature));
            }
        }

        // Comprobamos Quorum de evaluación
        let evaluator_quorum = self
            .governance
            .get_quorum(metadata, ValidationStage::Evaluate)
            .await
            .map_err(|_| ApprovalManagerError::GovernanceChannelFailed)?;

        match evaluation.acceptance {
            Acceptance::Ok => {
                if !(approval_request.proposal.evaluation_signatures.len() as u32 >= evaluator_quorum) {
                    return Ok(Err(ApprovalErrorResponse::NoQuorumReached));
                }
            }
            Acceptance::Ko => {
                let negativate_quorum = evaluators.len() as u32 - evaluator_quorum;
                if !(approval_request.proposal.evaluation_signatures.len() as u32 > negativate_quorum) {
                    return Ok(Err(ApprovalErrorResponse::NoQuorumReached));
                }
            }
            Acceptance::Error => return Ok(Err(ApprovalErrorResponse::InvalidAcceptance)),
        }

        // La EventRequest es correcta. Podemos pasar a guardarla en el sistema si corresponde
        // Esto dependerá del Flag PassVotation
        // - VotationType::Normal => Se guarda en el sistema a espera del usuario
        // - VotarionType::AlwaysAccept => Se emite voto afirmativo
        // - VotarionType::AlwaysReject => Se emite voto negativo

        let approval_petition_data = ApprovalPetitionData {
            subject_id: subject_data.subject_id.clone(),
            sn: approval_request.proposal.sn,
            governance_id: subject_data.governance_id,
            governance_version: version,
            hash_event_proporsal: approval_request
                .subject_signature
                .content
                .event_content_hash
                .clone(),
            sender: subject_data.owner.clone(),
            json_patch: approval_request.proposal.json_patch.clone()
        };

        self.subject_been_approved
            .insert(subject_data.subject_id.clone(), id.clone());
        self.request_table
            .insert(id.clone(), approval_petition_data);
        self.notifier
            .request_reached(&id.to_str(), &subject_data.subject_id.to_str());

        match self.pass_votation {
            VotationType::Normal => return Ok(Ok(None)),
            VotationType::AlwaysAccept => {
                let (vote, sender) = self
                    .generate_vote(&id, Acceptance::Ok)
                    .await?
                    .expect("Request should be in data structure");
                return Ok(Ok(Some((vote, sender))));
            }
            VotationType::AlwaysReject => {
                let (vote, sender) = self
                    .generate_vote(&id, Acceptance::Ko)
                    .await?
                    .expect("Request should be in data structure");
                return Ok(Ok(Some((vote, sender))));
            }
        }
    }

    fn check_event_request_signatures(
        &self,
        event_request: &EventRequest,
    ) -> Result<Result<(), ApprovalErrorResponse>, ApprovalManagerError> {
        let hash_request = DigestIdentifier::from_serializable_borsh((
            &event_request.request,
            &event_request.timestamp,
        ))
        .map_err(|_| ApprovalManagerError::HashGenerationFailed)?;
        // Check that the hash is the same
        if hash_request != event_request.signature.content.event_content_hash {
            return Ok(Err(ApprovalErrorResponse::NoHashCorrelation));
        }
        // Check that the signature matches the hash
        match event_request.signature.content.signer.verify(
            &hash_request.derivative(),
            &event_request.signature.signature,
        ) {
            Ok(_) => return Ok(Ok(())),
            Err(_) => {
                return Ok(Err(ApprovalErrorResponse::InvalidInvokator));
            }
        };
    }

    pub async fn generate_vote(
        &mut self,
        request_id: &DigestIdentifier,
        acceptance: Acceptance,
    ) -> Result<Result<(Approval, KeyIdentifier), ApprovalErrorResponse>, ApprovalManagerError>
    {
        // Obtenemos la petición
        let Some(data) = self.request_table.get(&request_id).cloned() else {
            return Ok(Err(ApprovalErrorResponse::ApprovalRequestNotFound));
        };

        let signature = self
            .signature_manager
            .sign(&(&data.hash_event_proporsal, &acceptance))
            .map_err(|_| ApprovalManagerError::SignProcessFailed)?;
        // Podría ser necesario un ACK
        self.request_table.remove(request_id);
        self.subject_been_approved.remove(&data.subject_id);
        Ok(Ok((
            Approval {
                content: ApprovalContent {
                    event_proposal_hash: data.hash_event_proporsal,
                    acceptance,
                },
                signature,
            },
            data.sender,
        )))
    }
}

fn event_proposal_hash_gen(
    approval_request: &EventProposal,
) -> Result<DigestIdentifier, ApprovalManagerError> {
    Ok(DigestIdentifier::from_serializable_borsh(&approval_request)
        .map_err(|_| ApprovalManagerError::HashGenerationFailed)?)
}

fn create_metadata(subject_data: &Subject, governance_version: u64) -> Metadata {
    Metadata {
        namespace: subject_data.namespace.clone(),
        subject_id: subject_data.subject_id.clone(),
        governance_id: subject_data.governance_id.clone(),
        governance_version,
        schema_id: subject_data.schema_id.clone(),
        owner: subject_data.owner.clone(),
        creator: subject_data.creator.clone(),
    }
}

/*
#[cfg(test)]
mod test {
    use std::{collections::HashSet, str::FromStr, sync::Arc};

    use async_trait::async_trait;
    use serde_json::Value;
    use tokio::{runtime::Runtime, sync::broadcast::Receiver};

    use crate::{
        approval::RequestApproval,
        commons::{
            crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair, Payload, DSA},
            models::state::Subject,
            schema_handler::gov_models::{Contract, Invoke},
        },
        database::DB,
        event_content::Metadata,
        event_request::{CreateRequest, EventRequest, EventRequestType, StateRequest},
        governance::{error::RequestError, stage::ValidationStage, GovernanceInterface},
        identifier::{Derivable, DigestIdentifier, KeyIdentifier, SignatureIdentifier},
        protocol::{
            command_head_manager::self_signature_manager::{
                SelfSignatureInterface, SelfSignatureManager,
            },
            request_manager::VotationType,
        },
        signature::{Signature, SignatureContent},
        Event, MemoryManager, Notification, TimeStamp,
    };

    use super::{InnerApprovalManager, RequestNotifier};

    struct GovernanceMockup {}

    #[async_trait]
    impl GovernanceInterface for GovernanceMockup {
        async fn get_schema(
            &self,
            governance_id: &DigestIdentifier,
            schema_id: String,
        ) -> Result<serde_json::Value, RequestError> {
            unreachable!()
        }

        async fn get_signers(
            &self,
            metadata: &Metadata,
            stage: ValidationStage,
        ) -> Result<HashSet<KeyIdentifier>, RequestError> {
            match stage {
                ValidationStage::Evaluate => {
                    Ok(HashSet::from_iter(vec![
                        // 63e9cd4c2592a7a0661b5a802c2b61a557c59b66bd5ab93e22cdcb4e0190b5d2
                        KeyIdentifier::from_str("EbJGKLvlNH4fO23sdGWYLipmab0BBOHH0yswlkQXTl08")
                            .unwrap(),
                    ]))
                }
                ValidationStage::Approve => {
                    Ok(HashSet::from_iter(vec![
                        // cd32a887e2e6446f2b91c44c612b9fa5e3a9ad536ed2461a03bfee947809a9d6
                        KeyIdentifier::from_str("Eo4dSHfIc5uB8AMsL0Q4F-kHKCkTXbEp1AzQzZ6rrL4g")
                            .unwrap(),
                    ]))
                }
                _ => unreachable!(),
            }
        }

        async fn get_quorum(
            &self,
            metadata: &Metadata,
            stage: ValidationStage,
        ) -> Result<u32, RequestError> {
            Ok(1)
        }

        async fn get_invoke_info(
            &self,
            metadata: &Metadata,
            fact: String,
        ) -> Result<Option<Invoke>, RequestError> {
            unreachable!()
        }

        async fn get_contracts(
            &self,
            governance_id: &DigestIdentifier,
        ) -> Result<Vec<Contract>, RequestError> {
            unreachable!()
        }

        async fn get_governance_version(
            &self,
            governance_id: &DigestIdentifier,
        ) -> Result<u64, RequestError> {
            Ok(1)
        }

        async fn is_governance(&self, subject_id: &DigestIdentifier) -> Result<bool, RequestError> {
            unreachable!()
        }
    }

    fn create_state_request(
        json: String,
        signature_manager: &SelfSignatureManager,
        subject_id: &DigestIdentifier,
    ) -> EventRequest {
        let request = EventRequestType::State(StateRequest {
            subject_id: subject_id.clone(),
            invokation: json,
        });
        let timestamp = TimeStamp::now();
        let signature = signature_manager.sign(&(&request, &timestamp)).unwrap();
        let event_request = EventRequest {
            request,
            timestamp,
            signature,
        };
        event_request
    }

    fn create_subject(
        request: EventRequest,
        governance_version: u64,
        subject_schema: &Value,
    ) -> (Subject, Event) {
        request
            .create_subject_from_request(governance_version, subject_schema, true)
            .unwrap()
    }

    fn create_genesis_request(
        json: String,
        signature_manager: &SelfSignatureManager,
    ) -> EventRequest {
        let request = EventRequestType::Create(CreateRequest {
            governance_id: DigestIdentifier::from_str(
                "J6axKnS5KQjtMDFgapJq49tdIpqGVpV7SS4kxV1iR10I",
            )
            .unwrap(),
            schema_id: "test".to_owned(),
            namespace: "test".to_owned(),
        });
        let timestamp = TimeStamp::now();
        let signature = signature_manager.sign(&(&request, &timestamp)).unwrap();
        let event_request = EventRequest {
            request,
            timestamp,
            signature,
        };
        event_request
    }

    fn create_json_state() -> String {
        serde_json::to_string(&serde_json::json!({"a": "test"})).unwrap()
    }

    fn create_subject_schema() -> Value {
        serde_json::json!({"a": {"type": "string"}})
    }

    fn create_evaluator_signature_manager() -> SelfSignatureManager {
        // key identifier: EceWPmTsy2oXYsAhnWqTpBKtpobsnWM0QT8sNUTtV_Pw
        let keypair = KeyPair::Ed25519(Ed25519KeyPair::from_seed(
            &hex::decode("1b40feb901fdbc5ded2e4ceb03f64a9365f38f0b2ab8019eb05fd5ebcb9bf0ef")
                .unwrap(),
        ));
        let pk = keypair.public_key_bytes();
        SelfSignatureManager {
            keys: keypair,
            identifier: KeyIdentifier::new(crate::KeyDerivator::Ed25519, &pk),
            digest_derivator: crate::DigestDerivator::Blake3_256,
        }
    }

    fn create_invokator_signature_manager() -> SelfSignatureManager {
        // key identifier: EUMju4Zy0RebWInQmzreZd8hox0z0RjDVWXMAi6oknl4
        let keypair = KeyPair::Ed25519(Ed25519KeyPair::from_seed(
            &hex::decode("613e38b1ea78d3d9e4b5f23910120efa5156cc1d78ade09e8edb21d741f97075")
                .unwrap(),
        ));
        let pk = keypair.public_key_bytes();
        SelfSignatureManager {
            keys: keypair,
            identifier: KeyIdentifier::new(crate::KeyDerivator::Ed25519, &pk),
            digest_derivator: crate::DigestDerivator::Blake3_256,
        }
    }

    fn generate_evaluator_signature(
        signature_manager: &SelfSignatureManager,
        success: bool,
        approval_required: bool,
        governance_version: u64,
    ) -> Signature {
        signature_manager
            .sign(&(
                DigestIdentifier::from_str("").unwrap(),
                DigestIdentifier::from_str("").unwrap(),
                governance_version,
                success,
                approval_required,
            ))
            .unwrap()
    }

    fn generate_request_approve_msg(
        request: EventRequest,
        sn: u64,
        governance_id: &DigestIdentifier,
        governance_version: u64,
        success: bool,
        evaluator_signatures: Vec<Signature>,
        subject: &Subject,
        json_patch: String,
    ) -> RequestApproval {
        let hash: DigestIdentifier = DigestIdentifier::from_serializable_borsh(&(
            &request,
            sn,
            DigestIdentifier::from_str("").unwrap(),
            DigestIdentifier::from_str("").unwrap(),
            &governance_id,
            governance_version,
            success,
            true,
            &evaluator_signatures,
            json_patch,
        ))
        .unwrap();
        let keys = subject.keys.as_ref().unwrap();
        let identifier = KeyIdentifier::new(keys.get_key_derivator(), &keys.public_key_bytes());
        let signature = keys.sign(Payload::Buffer(hash.derivative())).unwrap();
        RequestApproval {
            request,
            sn,
            context_hash: DigestIdentifier::from_str("").unwrap(),
            hash_new_state: DigestIdentifier::from_str("").unwrap(),
            governance_id: governance_id.clone(),
            governance_version,
            success,
            approval_required: true,
            evaluator_signatures,
            json_patch,
            subject_signature: Signature {
                content: SignatureContent {
                    signer: identifier.clone(),
                    event_content_hash: hash,
                    timestamp: TimeStamp::now(),
                },
                signature: SignatureIdentifier::new(
                    identifier.to_signature_derivator(),
                    &signature,
                ),
            },
        }
    }

    fn create_module(
        pass_votation: VotationType,
    ) -> (
        InnerApprovalManager<GovernanceMockup, MemoryManager, RequestNotifier>,
        Receiver<Notification>,
        Arc<MemoryManager>,
        SelfSignatureManager,
    ) {
        let database = Arc::new(MemoryManager::new());
        let keypair = KeyPair::Ed25519(Ed25519KeyPair::from_seed(
            &hex::decode("99beed715bf561185baaa5b3e9df8ecddcfcf7727fbc4f7e922a4cf2f9ea8c4e")
                .unwrap(),
        ));
        let pk = keypair.public_key_bytes();
        let signature_manager = SelfSignatureManager {
            keys: keypair,
            identifier: KeyIdentifier::new(crate::KeyDerivator::Ed25519, &pk),
            digest_derivator: crate::DigestDerivator::Blake3_256,
        };
        let governance = GovernanceMockup {};
        let (notification_sx, notification_rx) = tokio::sync::broadcast::channel(100);
        let notifier = RequestNotifier::new(notification_sx);
        let manager = InnerApprovalManager::new(
            governance,
            DB::new(database.clone()),
            notifier,
            signature_manager.clone(),
            pass_votation,
        );
        (manager, notification_rx, database, signature_manager)
    }

    fn get_governance_id() -> DigestIdentifier {
        DigestIdentifier::from_str("J6axKnS5KQjtMDFgapJq49tdIpqGVpV7SS4kxV1iR10I").unwrap()
    }

    #[test]
    fn subject_not_found_test() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (manager, not_rx, database, signature_manager) =
                create_module(VotationType::AlwaysAccept);
            // Creamos los datos
            let invokator = create_invokator_signature_manager();
            let (subject, _) = create_subject(
                create_genesis_request(create_json_state(), &invokator),
                0,
                &create_subject_schema(),
            );
            let subject_data = subject.subject_data.as_ref().unwrap();
            let msg = generate_request_approve_msg(
                create_state_request(create_json_state(), &invokator, &subject_data.subject_id),
                1,
                &get_governance_id(),
                0,
                true,
                vec![generate_evaluator_signature(
                    &create_evaluator_signature_manager(),
                    true,
                    true,
                    0,
                )],
                &subject,
                "".into(),
            );

            // let result = manager.
        });
    }
}
 */