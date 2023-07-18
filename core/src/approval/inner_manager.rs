use crate::{
    commons::{
        config::VotationType,
        models::{
            approval::{ ApprovalResponse, ApprovalState, ApprovalEntity},
            state::{Subject, generate_subject_id}, event::Metadata,
        },
        self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
    },
    database::DB,
    request::{ EventRequest},
    governance::{error::RequestError, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    DatabaseCollection, Notification, signature::Signed, ApprovalRequest,
};

use super::{
    error::{ApprovalErrorResponse, ApprovalManagerError},
};

pub trait NotifierInterface {
    fn request_reached(&self, id: &str, subject_id: &str, sn: u64);
    fn request_obsolete(&self, id: String, subject_id: String, sn: u64);
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
    fn request_reached(&self, id: &str, subject_id: &str, sn: u64) {
        let _ = self.sender.send(Notification::ApprovalReceived {
            id: id.to_owned(),
            subject_id: subject_id.to_owned(),
            sn
        });
    }

    fn request_obsolete(&self, id: String, subject_id: String, sn: u64) {
        let _ = self.sender.send(Notification::ObsoletedApproval {
            id: id,
            subject_id: subject_id,
            sn
        });
    }
}

pub struct InnerApprovalManager<G: GovernanceInterface, N: NotifierInterface, C: DatabaseCollection>
{
    governance: G,
    database: DB<C>,
    notifier: N,
    signature_manager: SelfSignatureManager,
    // Cola de 1 elemento por sujeto
    // subject_been_approved: HashMap<DigestIdentifier, DigestIdentifier>, // SubjectID -> ReqId
    pass_votation: VotationType,
}

impl<G: GovernanceInterface, N: NotifierInterface, C: DatabaseCollection>
    InnerApprovalManager<G, N, C>
{
    pub fn new(
        governance: G,
        database: DB<C>,
        notifier: N,
        signature_manager: SelfSignatureManager,
        pass_votation: VotationType,
    ) -> Self {
        Self {
            governance,
            database,
            notifier,
            signature_manager,
            // subject_been_approved: HashMap::new(),
            pass_votation,
        }
    }

    pub fn get_single_request(
        &self,
        request_id: &DigestIdentifier,
    ) -> Result<ApprovalEntity, ApprovalErrorResponse> {
        let request = self
            .database
            .get_approval(request_id)
            .map_err(|_| ApprovalErrorResponse::ApprovalRequestNotFound)?;
        Ok(request)
    }

    pub fn get_all_request(&self) -> Vec<ApprovalEntity> {
        self.database
            .get_approvals(Some(ApprovalState::Pending), None, isize::MAX)
            .unwrap()
    }

    #[allow(dead_code)]
    pub fn change_pass_votation(&mut self, pass_votation: VotationType) {
        self.pass_votation = pass_votation;
    }

    pub async fn get_governance_version(
        &self,
        governance_id: &DigestIdentifier,
        subject_id: &DigestIdentifier,
    ) -> Result<Result<u64, ApprovalErrorResponse>, ApprovalManagerError> {
        match self
            .governance
            .get_governance_version(governance_id.to_owned(), subject_id.clone())
            .await
        {
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
    ) -> Result<(), ApprovalManagerError> {
        // Comprobamos todas las peticiones guardadas y borramos las afectadas
        let affected_requests = self.database.get_approvals_by_governance(governance_id)
            .map_err(|_| ApprovalManagerError::DatabaseError)?;
        for request in affected_requests {
            // Borrarlas de la colección principal y del índice
            let approval_entity = self.database.get_approval(&request).map_err(|_| ApprovalManagerError::DatabaseError)?;
            let subject_id = {
                match approval_entity.request.content.event_request.content {
                    EventRequest::Fact(ref fact_request) => fact_request.subject_id.clone(),
                    EventRequest::Create(ref create_request) => generate_subject_id(&create_request.namespace, &create_request.schema_id, create_request.public_key.to_str(), create_request.governance_id.to_str(), approval_entity.request.content.gov_version).map_err(|_| ApprovalManagerError::UnexpectedError)?,
                    _ => return Err(ApprovalManagerError::UnexpectedRequestType),
                }
            };
            self.notifier.request_obsolete(approval_entity.id.to_str(), subject_id.to_str(), approval_entity.request.content.sn);
            self.database.del_approval(&request).map_err(|_| ApprovalManagerError::DatabaseError)?;
            self.database.del_governance_aproval_index(&governance_id, &request).map_err(|_| ApprovalManagerError::DatabaseError)?;
            self.database.del_subject_aproval_index(&subject_id, &request).map_err(|_| ApprovalManagerError::DatabaseError)?;
        }
        Ok(())
    }

    pub async fn process_approval_request(
        &mut self,
        approval_request: Signed<ApprovalRequest>,
        sender: KeyIdentifier,
    ) -> Result<
        Result<Option<(Signed<ApprovalResponse>, KeyIdentifier)>, ApprovalErrorResponse>,
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
        let id: DigestIdentifier = match DigestIdentifier::from_serializable_borsh(&approval_request.content).map_err(|_| ApprovalErrorResponse::ErrorHashing) {
            Ok(id) => id,
            Err(error) => return Ok(Err(error)),
        };

        if let Ok(data) = self.get_single_request(&id) {
            match data.state {
                ApprovalState::Pending | ApprovalState::Obsolete => return Ok(Err(ApprovalErrorResponse::RequestAlreadyKnown)),
                ApprovalState::Responded => {
                    let result = self
                    .generate_vote(&id, data.response.expect("Should be").content.approved)
                    .await?;
                let (vote, sender) = result
                .expect("Request should be in data structure");
                return Ok(Ok(Some((vote.response.unwrap(), sender))))
                },
            }
        };
        // Comprobamos si tenemos el sujeto y si estamos sincronizados
        // let mut subject_data = match self.database.get_subject(&state_request.subject_id) {
        //     Ok(subject) => subject,
        //     Err(DbError::EntryNotFound) => return Ok(Err(ApprovalErrorResponse::SubjectNotFound)),
        //     Err(_error) => return Err(ApprovalManagerError::DatabaseError),
        // };

        // log::error!("PARTE 4");
        // if approval_request.content.sn > subject_data.sn + 1 {
        //     return Ok(Err(ApprovalErrorResponse::SubjectNotSynchronized));
        // }

        // Comprobamos si ya estamos aprobando el sujeto para un evento igual o mayor.
        // En caso de no haber request previa, continuamos.
        let subject_id = subject_id_by_request(&approval_request.content.event_request.content, approval_request.content.gov_version)?;
        let request_queue = self.database.get_approvals_by_subject(&subject_id).map_err(|_| ApprovalManagerError::DatabaseError)?;
        if request_queue.len() == 1 {
            let data = self.get_single_request(&request_queue[0]).unwrap();
            if approval_request.content.sn <= data.request.content.sn {
                return Ok(Err(ApprovalErrorResponse::PreviousEventDetected));
            }
        } else if request_queue.len() != 0 {
            return Err(ApprovalManagerError::MoreRequestThanMaxAllowed);
        }

        // Comprobamos si la versión de la gobernanza es correcta
        let version = match self
            .get_governance_version(&approval_request.content.gov_id, &subject_id)
            .await?
        {
            Ok(version) => version,
            Err(error) => return Ok(Err(error)),
        };

        let request_gov_version = approval_request.content.gov_version;

        if version > request_gov_version {
            // Nuestra gov es mayor: mandamos mensaje para que actualice el emisor
            return Ok(Err(ApprovalErrorResponse::OurGovIsHigher {
                our_id: self.signature_manager.get_own_identifier(),
                sender,
                gov_id: approval_request.content.gov_id.clone(),
            }));
        } else if version < request_gov_version {
            // Nuestra gov es menor: no podemos hacer nada. Pedimos LCE al que nos lo envió
            return Ok(Err(ApprovalErrorResponse::OurGovIsLower {
                our_id: self.signature_manager.get_own_identifier(),
                sender,
                gov_id: approval_request.content.gov_id.clone(),
            }));
        }
        // log::error!("PARTE 6");
        // let metadata = create_metadata(&subject_data, version);

        // // Comprobar si somos aprobadores. Esto antes incluso que la firma del sujeto
        // let approvers_list = self
        //     .governance
        //     .get_signers(metadata.clone(), ValidationStage::Approve)
        //     .await
        //     .map_err(|_| ApprovalManagerError::GovernanceChannelFailed)?;
        // let current_node = self.signature_manager.get_own_identifier();
        // if !approvers_list.contains(&current_node) {
        //     return Ok(Err(ApprovalErrorResponse::NodeIsNotApprover));
        // }
        // log::error!("PARTE 7");
        // // Comprobamos validez criptográfica de la firma del sujeto
        // // Empezamos comprobando que el firmante sea el sujeto
        // if approval_request.signature.signer != subject_data.public_key {
        //     return Ok(Err(ApprovalErrorResponse::SignatureSignerIsNotSubject));
        // }

        // // Verificamos la firma
        // let Ok(()) = approval_request.signature.verify(&approval_request.content) else {
        //     return Ok(Err(ApprovalErrorResponse::InvalidSubjectSignature));
        // };

        // Tenemos que realizar un falso apply para comprobar si el state_hash es correcto
        // subject_data.update_subject(approval_request.content.patch.clone(), subject_data.sn + 1).map_err(|_| ApprovalManagerError::EventApplyFailed)?;
        
        // let hash_state = match DigestIdentifier::from_serializable_borsh(&subject_data.properties).map_err(|_| ApprovalErrorResponse::ErrorHashing) {
        //     Ok(id) => id,
        //     Err(error) => return Ok(Err(error)),
        // };

        // if approval_request.content.state_hash != hash_state {
        //     return Ok(Err(ApprovalErrorResponse::InvalidStateHashAfterApply))
        // }
        
        // La EventRequest es correcta. Podemos pasar a guardarla en el sistema si corresponde
        // Esto dependerá del Flag PassVotation
        // - VotationType::Normal => Se guarda en el sistema a espera del usuario
        // - VotarionType::AlwaysAccept => Se emite voto afirmativo
        // - VotarionType::AlwaysReject => Se emite voto negativo
        let gov_id = approval_request.content.gov_id.clone();
        let sn = approval_request.content.sn;
        let approval_entity = ApprovalEntity {
            id: id.clone(),
            request: approval_request,
            response: None,
            state: ApprovalState::Pending,
            sender,
        };
        self.database.set_subject_aproval_index(
            &subject_id,
            &id)
        .map_err(|_| ApprovalManagerError::DatabaseError)?;
        if !gov_id.digest.is_empty() {
            self.database.set_governance_aproval_index(
                &gov_id,
                &id)
            .map_err(|_| ApprovalManagerError::DatabaseError)?;   
        }
        let Ok(_result) = self.database.set_approval(&id, approval_entity) else { 
            return Err(ApprovalManagerError::DatabaseError);
        };
        self.notifier
            .request_reached(&id.to_str(), &subject_id.to_str(), sn);

        match self.pass_votation {
            VotationType::Normal => return Ok(Ok(None)),
            VotationType::AlwaysAccept => {
                let (vote, sender) = self
                    .generate_vote(&id, true)
                    .await?
                    .expect("Request should be in data structure");
                return Ok(Ok(Some((vote.response.unwrap(), sender))));
            }
            VotationType::AlwaysReject => {
                let (vote, sender) = self
                    .generate_vote(&id, false)
                    .await?
                    .expect("Request should be in data structure");
                return Ok(Ok(Some((vote.response.unwrap(), sender))));
            }
        }
    }

    pub async fn generate_vote(
        &mut self,
        request_id: &DigestIdentifier,
        acceptance: bool,
    ) -> Result<Result<(ApprovalEntity, KeyIdentifier), ApprovalErrorResponse>, ApprovalManagerError> {
        // Obtenemos la petición
        let Ok(mut data) = self.get_single_request(&request_id) else {
            return Ok(Err(ApprovalErrorResponse::RequestNotFound));
        };
        let response = ApprovalResponse {
            appr_req_hash: request_id.clone(),
            approved: acceptance,
        };
        let subject_id = subject_id_by_request(&data.request.content.event_request.content, data.request.content.gov_version)?;
        let signature = self
            .signature_manager
            .sign(&response)
            .map_err(|_| ApprovalManagerError::SignProcessFailed)?;
        data.state = ApprovalState::Responded;
        data.response = Some(Signed::<ApprovalResponse> {
            content: response,
            signature,
        });
        let Ok(_result) = self.database.set_approval(&request_id, data.clone()) else {
            return Err(ApprovalManagerError::DatabaseError)
        };
        self.database.del_subject_aproval_index(&subject_id, request_id).map_err(|_| ApprovalManagerError::DatabaseError)?;
        self.database.del_governance_aproval_index(&data.request.content.gov_id, request_id).map_err(|_| ApprovalManagerError::DatabaseError)?;
        let sender = data.sender.clone();
        Ok(Ok((data, sender)))
    }
}

#[allow(dead_code)]
fn event_proposal_hash_gen(
    approval_request: &Signed<ApprovalRequest>,
) -> Result<DigestIdentifier, ApprovalManagerError> {
    Ok(DigestIdentifier::from_serializable_borsh(approval_request)
        .map_err(|_| ApprovalManagerError::HashGenerationFailed)?)
}

#[allow(dead_code)]
fn create_metadata(subject_data: &Subject, governance_version: u64) -> Metadata {
    Metadata {
        namespace: subject_data.namespace.clone(),
        subject_id: subject_data.subject_id.clone(),
        governance_id: subject_data.governance_id.clone(),
        governance_version,
        schema_id: subject_data.schema_id.clone(),
    }
}

fn subject_id_by_request(request: &EventRequest, gov_version: u64) -> Result<DigestIdentifier, ApprovalManagerError> {
        let subject_id = match request {
            EventRequest::Fact(ref fact_request) => fact_request.subject_id.clone(),
            EventRequest::Create(ref create_request) => generate_subject_id(&create_request.namespace, &create_request.schema_id, create_request.public_key.to_str(), create_request.governance_id.to_str(), gov_version).map_err(|_| ApprovalManagerError::UnexpectedError)?,
            _ => return Err(ApprovalManagerError::UnexpectedRequestType),
        };
        Ok(subject_id)
}

/*
#[cfg(test)]
mod test {
    use std::{collections::HashSet, str::FromStr, sync::Arc};

    use async_trait::async_trait;
    use serde_json::Value;
    use tokio::{sync::broadcast::Receiver};

    use crate::{
        approval::RequestApproval,
        commons::{
            config::VotationType,
            crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair, Payload, DSA},
            models::{state::Subject, timestamp, value_wrapper::ValueWrapper, request::{StartRequest, FactRequest}},
            schema_handler::gov_models::Contract,
            self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
        },
        database::{MemoryCollection, DB},
        event_content::Metadata,
        governance::{error::RequestError, stage::ValidationStage, GovernanceInterface},
        identifier::{DigestIdentifier, KeyIdentifier},
        signature::{Signature, Signed},
        DatabaseManager, MemoryManager, Notification, TimeStamp, EventRequest,
    };

    use super::{InnerApprovalManager, RequestNotifier};

    struct GovernanceMockup {}

    #[async_trait]
    impl GovernanceInterface for GovernanceMockup {
        async fn get_schema(
            &self,
            governance_id: DigestIdentifier,
            schema_id: String,
            governance_version: u64,
        ) -> Result<ValueWrapper, RequestError> {
            unreachable!()
        }

        async fn get_signers(
            &self,
            metadata: Metadata,
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
            metadata: Metadata,
            stage: ValidationStage,
        ) -> Result<u32, RequestError> {
            Ok(1)
        }

        async fn get_invoke_info(
            &self,
            metadata: Metadata,
            stage: ValidationStage,
            invoker: KeyIdentifier,
        ) -> Result<bool, RequestError> {
            unreachable!()
        }

        async fn get_contracts(
            &self,
            governance_id: DigestIdentifier,
            governance_version: u64,
        ) -> Result<Vec<(Contract, String)>, RequestError> {
            unreachable!()
        }

        async fn get_governance_version(
            &self,
            governance_id: DigestIdentifier,
            subject_id: DigestIdentifier,
        ) -> Result<u64, RequestError> {
            Ok(1)
        }

        async fn is_governance(&self, subject_id: DigestIdentifier) -> Result<bool, RequestError> {
            unreachable!()
        }

        async fn get_init_state(
            &self,
            governance_id: DigestIdentifier,
            schema_id: String,
            governance_version: u64,
        ) -> Result<ValueWrapper, RequestError> {
            unreachable!()
        }

        async fn governance_updated(
            &self,
            governance_id: DigestIdentifier,
            governance_version: u64,
        ) -> Result<(), RequestError> {
            unreachable!()
        }
    }

    fn create_state_request(
        json: ValueWrapper,
        signature_manager: &SelfSignatureManager,
        subject_id: &DigestIdentifier,
    ) -> Signed<EventRequest> {
        let request = EventRequest::Fact(FactRequest {
            subject_id: subject_id.clone(),
            payload: json,
        });
        let signature = signature_manager.sign(&request).unwrap(); // TODO: MAL usar Signature::new
        let event_request = Signed::<EventRequest>::new(request, signature);
        event_request
    }

    fn create_genesis_request(
        json: String,
        signature_manager: &SelfSignatureManager,
    ) -> Signed<EventRequest> {
        let request = EventRequest::Create(StartRequest {
            governance_id: DigestIdentifier::from_str(
                "J6axKnS5KQjtMDFgapJq49tdIpqGVpV7SS4kxV1iR10I",
            )
            .unwrap(),
            schema_id: "test".to_owned(),
            namespace: "test".to_owned(),
            name: "test".to_owned(),
            public_key: KeyIdentifier::from_str("EceWPmTsy2oXYsAhnWqTpBKtpobsnWM0QT8sNUTtV_Pw")
                .unwrap(), // TODO: Revisar, lo puse a voleo
        });
        let signature = signature_manager.sign(&request).unwrap(); // TODO: MAL usar Signature::new
        let event_request = Signed::<EventRequest>::new(request, signature);
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
        request: Signed<EventRequest>,
        sn: u64,
        governance_id: &DigestIdentifier,
        governance_version: u64,
        success: bool,
        evaluator_signatures: Vec<Signature>,
        subject: &Subject,
        json_patch: String,
    ) -> RequestApproval {
        let content_hash: DigestIdentifier = DigestIdentifier::from_serializable_borsh(&(
            &request,
            sn,
            DigestIdentifier::from_str("").unwrap(),
            DigestIdentifier::from_str("").unwrap(),
            &governance_id,
            governance_version,
            success,
            true,
            &evaluator_signatures,
            json_patch.clone(),
        ))
        .unwrap();
        let keys = subject.keys.as_ref().unwrap();
        let identifier = KeyIdentifier::new(keys.get_key_derivator(), &keys.public_key_bytes());
        let subject_signature = Signature::new(
            &(
                &request,
                sn,
                DigestIdentifier::from_str("").unwrap(),
                DigestIdentifier::from_str("").unwrap(),
                &governance_id,
                governance_version,
                success,
                true,
                &evaluator_signatures,
                json_patch.clone(),
            ),
            identifier.clone(),
            &keys,
        )
        .unwrap();
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
            subject_signature,
        }
    }

    fn create_module(
        pass_votation: VotationType,
    ) -> (
        InnerApprovalManager<GovernanceMockup, RequestNotifier, MemoryCollection>,
        Receiver<Notification>,
        Arc<MemoryManager>,
        SelfSignatureManager,
    ) {
        let collection = Arc::new(MemoryManager::new());
        let database = DB::new(collection.clone());
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
            database,
            notifier,
            signature_manager.clone(),
            pass_votation,
        );
        (manager, notification_rx, collection, signature_manager)
    }

    fn get_governance_id() -> DigestIdentifier {
        DigestIdentifier::from_str("J6axKnS5KQjtMDFgapJq49tdIpqGVpV7SS4kxV1iR10I").unwrap()
    }

    // #[test]
    // fn subject_not_found_test() {
    //     let rt = Runtime::new().unwrap();
    //     rt.block_on(async {
    //         let (manager, not_rx, database, signature_manager) =
    //             create_module(VotationType::AlwaysAccept);
    //         // Creamos los datos
    //         let invokator = create_invokator_signature_manager();
    //         let subject = Subject::from_genesis_request(
    //             create_genesis_request(create_json_state(), &invokator),
    //             create_json_state(),
    //         )
    //         .unwrap();
    //         let msg = generate_request_approve_msg(
    //             create_state_request(create_json_state(), &invokator, &subject.subject_id),
    //             1,
    //             &get_governance_id(),
    //             0,
    //             true,
    //             vec![generate_evaluator_signature(
    //                 &create_evaluator_signature_manager(),
    //                 true,
    //                 true,
    //                 0,
    //             )],
    //             &subject,
    //             "".into(),
    //         );
    //     });
    // }
}
 */