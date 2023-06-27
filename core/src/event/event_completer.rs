use std::collections::{HashMap, HashSet};

use json_patch::{patch, Patch};

use crate::{
    commons::{
        channel::SenderEnd,
        models::{
            approval::UniqueApproval,
            evaluation::{EvaluationRequest, SubjectContext},
            event::{Event},
            validation::ValidationProof,
            event_proposal::{Evaluation, ApprovalRequest},
            state::{generate_subject_id, Subject},
            Acceptance,
        },
        self_signature_manager::SelfSignatureManager,
    },
    crypto::{KeyMaterial, KeyPair},
    event_content::Metadata,
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    ledger::{LedgerCommand, LedgerResponse},
    message::{MessageConfig, MessageTaskCommand},
    notary::NotaryEvent,
    protocol::protocol_message_manager::TapleMessages,
    request::StartRequest,
    request::TapleRequest,
    signature::{Signature, Signed, UniqueSignature},
    utils::message::{
        approval::create_approval_request, evaluator::create_evaluator_request,
        ledger::request_gov_event, validation::create_validator_request,
    },
    ApprovalResponse, DatabaseCollection, EventRequest, Notification, ValueWrapper,
};
use std::hash::Hash;

use super::errors::EventError;
use crate::database::DB;

const TIMEOUT: u32 = 2000;
// const GET_ALL: isize = 200;
const QUORUM_PORCENTAGE_AMPLIFICATION: f64 = 0.2;

pub struct EventCompleter<C: DatabaseCollection> {
    gov_api: GovernanceAPI,
    database: DB<C>,
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    notification_sender: tokio::sync::broadcast::Sender<Notification>,
    ledger_sender: SenderEnd<LedgerCommand, LedgerResponse>,
    own_identifier: KeyIdentifier,
    subjects_by_governance: HashMap<DigestIdentifier, HashSet<DigestIdentifier>>,
    subjects_completing_event:
        HashMap<DigestIdentifier, (ValidationStage, HashSet<KeyIdentifier>, (u32, u32))>,
    // actual_sn: HashMap<DigestIdentifier, u64>,
    // virtual_state: HashMap<DigestIdentifier, Value>,
    // Evaluation HashMaps
    event_pre_evaluations: HashMap<DigestIdentifier, EvaluationRequest>,
    event_evaluations:
        HashMap<DigestIdentifier, HashSet<(UniqueSignature, Acceptance, DigestIdentifier)>>,
    // Approval HashMaps
    event_proposals: HashMap<DigestIdentifier, Signed<ApprovalRequest>>,
    event_approvations: HashMap<DigestIdentifier, HashSet<UniqueApproval>>,
    // Validation HashMaps
    events_to_validate: HashMap<DigestIdentifier, Signed<Event>>,
    event_validations: HashMap<DigestIdentifier, HashSet<UniqueSignature>>,
    event_notary_events: HashMap<DigestIdentifier, NotaryEvent>,
    // SignatureManager
    signature_manager: SelfSignatureManager,
}

impl<C: DatabaseCollection> EventCompleter<C> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<C>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        ledger_sender: SenderEnd<LedgerCommand, LedgerResponse>,
        own_identifier: KeyIdentifier,
        signature_manager: SelfSignatureManager,
    ) -> Self {
        Self {
            gov_api,
            database,
            message_channel,
            notification_sender,
            ledger_sender,
            subjects_completing_event: HashMap::new(),
            // actual_sn: HashMap::new(),
            // virtual_state: HashMap::new(),
            event_pre_evaluations: HashMap::new(),
            event_evaluations: HashMap::new(),
            event_proposals: HashMap::new(),
            events_to_validate: HashMap::new(),
            event_approvations: HashMap::new(),
            event_validations: HashMap::new(),
            subjects_by_governance: HashMap::new(),
            event_notary_events: HashMap::new(),
            own_identifier,
            signature_manager,
        }
    }

    fn create_notary_event_from_genesis(
        &self,
        create_request: StartRequest,
        event_hash: DigestIdentifier,
        governance_version: u64,
        subject_id: DigestIdentifier,
        subject_keys: &KeyPair,
    ) -> Result<NotaryEvent, EventError> {
        let validation_proof = ValidationProof::new_from_genesis_event(
            create_request,
            event_hash,
            governance_version,
            subject_id,
        );
        let public_key = KeyIdentifier::new(
            subject_keys.get_key_derivator(),
            &subject_keys.public_key_bytes(),
        );
        let subject_signature = Signature::new(&validation_proof, public_key, subject_keys)?;
        Ok(NotaryEvent {
            proof: validation_proof,
            subject_signature,
            previous_proof: None,
            prev_event_validation_signatures: HashSet::new(),
        })
    }

    fn create_notary_event(
        &self,
        subject: &Subject,
        event: &Signed<Event>,
        gov_version: u64,
    ) -> Result<NotaryEvent, EventError> {
        let prev_event_hash = if event.content.event_proposal.content.sn == 0 {
            DigestIdentifier::default()
        } else {
            DigestIdentifier::from_serializable_borsh(
                &self
                    .database
                    .get_event(
                        &subject.subject_id,
                        event.content.event_proposal.content.sn - 1,
                    )
                    .map_err(|e| EventError::DatabaseError(e.to_string()))?
                    .content,
            )
            .map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the proposal"))
            })?
        };
        let event_hash =
            DigestIdentifier::from_serializable_borsh(&event.content).map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the proposal"))
            })?;
        let proof = match &event.content.event_proposal.content.event_request.content {
            EventRequest::Create(_) | EventRequest::Fact(_) | EventRequest::EOL(_) => {
                ValidationProof::new(
                    subject,
                    event.content.event_proposal.content.sn,
                    prev_event_hash,
                    event_hash,
                    gov_version,
                )
            }
            EventRequest::Transfer(transfer_request) => ValidationProof::new_from_transfer_event(
                subject,
                event.content.event_proposal.content.sn,
                prev_event_hash,
                event_hash,
                gov_version,
                transfer_request.public_key.clone(),
            ),
        };
        let (prev_event_validation_signatures, previous_proof) = {
            let (prev_event_validation_signatures, previous_proof) = self
                .database
                .get_signatures(&subject.subject_id, subject.sn)
                .map_err(|e| {
                    EventError::DatabaseError(format!(
                        "Error getting the signatures of the previous event: {}",
                        e
                    ))
                })?;
            (prev_event_validation_signatures, Some(previous_proof))
        };
        match &subject.keys {
            Some(keys) => {
                let subject_signature = Signature::new(&proof, subject.public_key.clone(), keys)?;
                Ok(NotaryEvent {
                    proof,
                    subject_signature,
                    previous_proof,
                    prev_event_validation_signatures,
                })
            }
            None => Err(EventError::SubjectNotOwned(subject.subject_id.to_str()))?,
        }
    }

    pub async fn init(&mut self) -> Result<(), EventError> {
        // Fill actual_sn with the last sn of last event created (not necessarily validated) of each subject
        let subjects = self.database.get_all_subjects();
        for subject in subjects.iter() {
            // Comprobar si hay eventos más allá del sn del sujeto que indica que debemos pedir las validaciones porque aún está pendiente de validar
            match self.database.get_prevalidated_event(&subject.subject_id) {
                Ok(last_event) => {
                    let gov_version = self
                        .gov_api
                        .get_governance_version(
                            subject.governance_id.clone(),
                            subject.subject_id.clone(),
                        )
                        .await?;
                    let metadata = Metadata {
                        namespace: subject.namespace.clone(),
                        subject_id: subject.subject_id.clone(),
                        governance_id: subject.governance_id.clone(),
                        governance_version: gov_version, // Not needed
                        schema_id: subject.schema_id.clone(),
                    };
                    let stage = ValidationStage::Validate;
                    let (signers, quorum_size) =
                        self.get_signers_and_quorum(metadata, stage.clone()).await?;
                    let notary_event =
                        self.create_notary_event(subject, &last_event, gov_version)?;
                    let event_message = create_validator_request(notary_event.clone());
                    self.ask_signatures(
                        &subject.subject_id,
                        event_message,
                        signers.clone(),
                        quorum_size,
                    )
                    .await?;
                    let last_event_hash = DigestIdentifier::from_serializable_borsh(
                        &last_event.content,
                    )
                    .map_err(|_| {
                        EventError::CryptoError("Error generating last event hash".to_owned())
                    })?;
                    self.event_notary_events
                        .insert(last_event_hash.clone(), notary_event);
                    self.events_to_validate.insert(last_event_hash, last_event);
                    self.subjects_completing_event.insert(
                        subject.subject_id.clone(),
                        (stage, signers, (quorum_size, 0)),
                    );
                    continue;
                }
                Err(error) => match error {
                    crate::DbError::EntryNotFound => {}
                    _ => return Err(EventError::DatabaseError(error.to_string())),
                },
            }
            // Comprobar si hay requests en la base de datos que corresponden con eventos que aun no han llegado a la fase de validación y habría que reiniciar desde pedir evaluaciones
            match self.database.get_request(&subject.subject_id) {
                Ok(event_request) => {
                    log::warn!("PASA POR AQUÍ EN INIT");
                    self.new_event(event_request).await?;
                }
                Err(error) => match error {
                    crate::DbError::EntryNotFound => {}
                    _ => return Err(EventError::DatabaseError(error.to_string())),
                },
            }
            // self.virtual_state.insert(
            //     subject.subject_id.to_owned(),
            //     serde_json::from_str(&subject.properties).expect("This should be OK"),
            // );
            // let mut last_event_sn = subject.sn;
            // let mut post_validated_events = self
            //     .database
            //     .get_events_by_range(&subject.subject_id, Some((subject.sn + 1) as i64), GET_ALL)
            //     .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            // for event in post_validated_events.into_iter() {
            //     last_event_sn = event.event_proposal.sn;
            //     if event.execution {
            //         let vs = self.virtual_state.get_mut(&subject.subject_id).unwrap();
            //         let Ok(patch_json) = serde_json::from_str::<Patch>(&event.event_proposal.evaluation.json_patch) else {
            //             return Err(EventError::ErrorParsingJsonString(event.event_proposal.evaluation.json_patch));
            //         };
            //         let Ok(()) = patch(vs, &patch_json) else {
            //             return Err(EventError::ErrorApplyingPatch(event.event_proposal.evaluation.json_patch));
            //         };
            //     }
            // }
            // self.actual_sn
            //     .insert(subject.subject_id.to_owned(), last_event_sn);
        }
        Ok(())
    }

    pub async fn new_governance_version(
        &mut self,
        governance_id: DigestIdentifier,
        new_version: u64,
    ) -> Result<(), EventError> {
        log::info!("NEW GOVERNANCE VERSION");
        // Pedir event requests para cada subject_id del set y lanza new_event con ellas
        match self.subjects_by_governance.get(&governance_id).cloned() {
            Some(subjects_affected) => {
                for subject_id in subjects_affected.iter() {
                    match self.database.get_request(subject_id) {
                        Ok(event_request) => {
                            let EventRequest::Fact(state_request) = &event_request.content else {
                                return Err(EventError::GenesisInGovUpdate)
                            };
                            let subject_id = state_request.subject_id.clone();
                            self.subjects_completing_event.remove(&subject_id);
                            self.new_event(event_request).await?;
                            log::info!("NEW GOVERNANCE VERSION NEW EVENT CALLED REQUEST");
                        }
                        Err(error) => match error {
                            crate::DbError::EntryNotFound => {}
                            _ => {
                                return Err(EventError::DatabaseError(error.to_string()));
                            }
                        },
                    }
                    match self.database.get_prevalidated_event(subject_id) {
                        Ok(event_prevalidated) => {
                            // TODO: Cambiar si se tuviesen que validar los genesis
                            let subject = self
                                .database
                                .get_subject(subject_id)
                                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
                            let metadata = Metadata {
                                namespace: subject.namespace.clone(),
                                subject_id: subject_id.clone(),
                                governance_id: subject.governance_id.clone(),
                                governance_version: new_version,
                                schema_id: subject.schema_id.clone(),
                            };
                            let notary_event = self.create_notary_event(
                                &subject,
                                &event_prevalidated,
                                new_version,
                            )?;
                            let event_message = create_validator_request(notary_event.clone());
                            let stage = ValidationStage::Validate;
                            let (signers, quorum_size) =
                                self.get_signers_and_quorum(metadata, stage.clone()).await?;
                            log::warn!(
                                "GOV_UPDATED: START PIDIENDO FIRMAS DE VALIDACION PARA: {}",
                                subject.subject_id.to_str()
                            );
                            self.ask_signatures(
                                &subject_id,
                                event_message,
                                signers.clone(),
                                quorum_size,
                            )
                            .await?;
                            let event_prevalidated_hash =
                                DigestIdentifier::from_serializable_borsh(
                                    &event_prevalidated.content,
                                )
                                .map_err(|_| {
                                    EventError::CryptoError(
                                        "Error generating event prevalidated hash in NGV"
                                            .to_owned(),
                                    )
                                })?;
                            self.event_notary_events
                                .insert(event_prevalidated_hash, notary_event);
                            // Hacer update de fase por la que va el evento
                            self.subjects_completing_event
                                .insert(subject_id.clone(), (stage, signers, (quorum_size, 0)));
                        }
                        Err(error) => match error {
                            crate::DbError::EntryNotFound => {}
                            _ => {
                                return Err(EventError::DatabaseError(error.to_string()));
                            }
                        },
                    }
                }
            }
            None => {}
        }
        Ok(())
    }

    async fn process_transfer_or_eol_event(
        &mut self,
        event_request: Signed<EventRequest>,
        subject: Subject,
        gov_version: u64,
    ) -> Result<(), EventError> {
        let subject_id = subject.subject_id.clone();
        // Check if we already have an event for that subject
        let None = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::EventAlreadyInProgress);
        };
        let (event_proposal, proposal_hash) = self
            .generate_event_proposal(&event_request, &subject, gov_version)
            .await?;
        let metadata = Metadata {
            namespace: subject.namespace.clone(),
            subject_id: subject_id.clone(),
            governance_id: subject.governance_id.clone(),
            governance_version: gov_version,
            schema_id: subject.schema_id.clone(),
        };
        match &event_request.content {
            EventRequest::Transfer(tr) => {
                if event_request.signature.signer == self.own_identifier {
                    self.database
                        .get_keys(&tr.public_key)
                        .map_err(|_| EventError::OwnTransferKeysDbError)?;
                }
            }
            EventRequest::EOL(_) => {
                if subject.creator != event_request.signature.signer {
                    return Err(EventError::CloseNotAuthorized(
                        event_request.signature.signer.to_str(),
                    ));
                }
            }
            _ => unreachable!(),
        }
        // Añadir al hashmap para poder acceder a él cuando lleguen las firmas de los validadores
        let event = &self.create_event_prevalidated(
            event_proposal,
            HashSet::new(),
            &subject,
            true, // TODO: Consultar
        )?;
        let event_hash = DigestIdentifier::from_serializable_borsh(&event.content)
            .map_err(|_| EventError::CryptoError("Error generating event hash".to_owned()))?;
        log::error!("PRE NOTARY");
        let notary_event = self.create_notary_event(&subject, &event, gov_version)?;
        log::error!("POST NOTARY");
        let event_message = create_validator_request(notary_event.clone());
        self.event_notary_events.insert(event_hash, notary_event);
        //(ValidationStage::Validate, event_message)
        let stage = ValidationStage::Validate;
        let (signers, quorum_size) = self.get_signers_and_quorum(metadata, stage.clone()).await?;
        self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
            .await?;
        // Hacer update de fase por la que va el evento
        self.subjects_completing_event
            .insert(subject_id.clone(), (stage, signers, (quorum_size, 0)));
        Ok(())
    }

    async fn generate_event_proposal(
        &self,
        event_request: &Signed<EventRequest>,
        subject: &Subject,
        gov_version: u64,
    ) -> Result<(Signed<ApprovalRequest>, DigestIdentifier), EventError> {
        let hash_prev_event = DigestIdentifier::from_serializable_borsh(
            &self
                .database
                .get_event(&subject.subject_id, subject.sn)
                .map_err(|e| EventError::DatabaseError(e.to_string()))?
                .content,
        )
        .map_err(|_| {
            EventError::CryptoError("Error calculating the hash of the previous event".to_string())
        })?;
        let proposal = ApprovalRequest::new(
            event_request.clone(),
            subject.sn + 1,
            hash_prev_event,
            gov_version,
            None,
            serde_json::from_str("[]")
                .map_err(|_| EventError::CryptoError("Error parsing empty json".to_string()))?,
            HashSet::new(),
        );
        let proposal_hash = DigestIdentifier::from_serializable_borsh(&proposal).map_err(|_| {
            EventError::CryptoError(String::from("Error calculating the hash of the proposal"))
        })?;
        let subject_keys = subject
            .keys
            .clone()
            .expect("Llegados a aquí tenemos que ser owner");
        let subject_signature =
            Signature::new(&proposal, subject.public_key.clone(), &subject_keys).map_err(|_| {
                EventError::CryptoError(String::from("Error signing the hash of the proposal"))
            })?;
        Ok((
            Signed::<ApprovalRequest>::new(proposal, subject_signature),
            proposal_hash,
        ))
    }

    pub async fn pre_new_event(
        &mut self,
        event_request: Signed<EventRequest>,
    ) -> Result<DigestIdentifier, EventError> {
        // Check if the content is correct (signature, invoker, etc)
        // Signature check:
        event_request.verify().map_err(EventError::SubjectError)?;
        let request_id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| EventError::HashGenerationFailed)?;
        // Comprobamos si ya tenemos la request registrada en el sistema
        match self.database.get_taple_request(&request_id) {
            Ok(_) => {
                return Err(EventError::RequestAlreadyKnown);
            }
            Err(crate::DbError::EntryNotFound) => {}
            Err(error) => return Err(EventError::DatabaseError(error.to_string())),
        }
        self.new_event(event_request).await
    }

    /// Function that is called when a new event request arrives at the system, either invoked by the controller or externally
    pub async fn new_event(
        &mut self,
        event_request: Signed<EventRequest>,
    ) -> Result<DigestIdentifier, EventError> {
        let request_id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| EventError::HashGenerationFailed)?;
        if let EventRequest::Create(create_request) = &event_request.content {
            // Comprobar si es governance, entonces vale todo, si no comprobar que el invoker soy yo y puedo hacerlo
            if event_request.signature.signer != self.own_identifier {
                return Err(EventError::ExternalGenesisEvent);
            }
            log::warn!("PREKEYS");
            // Check if i have the keys
            let subject_keys = match self.database.get_keys(&create_request.public_key) {
                Ok(keys) => keys,
                Err(crate::DbError::EntryNotFound) => {
                    return Err(EventError::SubjectKeysNotFound(
                        create_request.public_key.to_str(),
                    ));
                }
                Err(error) => return Err(EventError::DatabaseError(error.to_string())),
            };
            log::warn!("SCHEMA ID: {}", create_request.schema_id);
            let (governance_version, initial_state) = if &create_request.schema_id != "governance" {
                log::warn!("LLEGA IF");
                let governance_version = self
                    .gov_api
                    .get_governance_version(
                        create_request.governance_id.clone(),
                        DigestIdentifier::default(),
                    )
                    .await
                    .map_err(EventError::GovernanceError)?;
                let creation_premission = self
                    .gov_api
                    .get_invoke_info(
                        Metadata {
                            namespace: create_request.namespace.clone(),
                            subject_id: DigestIdentifier::default(), // Not necessary for this method
                            governance_id: create_request.governance_id.clone(),
                            governance_version,
                            schema_id: create_request.schema_id.clone(),
                        },
                        ValidationStage::Create,
                        self.own_identifier.clone(),
                    )
                    .await
                    .map_err(EventError::GovernanceError)?;
                if !creation_premission {
                    return Err(EventError::CreatingPermissionDenied);
                }
                let initial_state = self
                    .gov_api
                    .get_init_state(
                        create_request.governance_id.clone(),
                        create_request.schema_id.clone(),
                        governance_version,
                    )
                    .await?;
                (governance_version, initial_state)
            } else {
                log::warn!("LLEGA ELSE");
                let initial_state = self
                    .gov_api
                    .get_init_state(
                        create_request.governance_id.clone(),
                        create_request.schema_id.clone(),
                        0,
                    )
                    .await?;
                (0, initial_state)
            };
            let subject_id = generate_subject_id(
                &create_request.namespace,
                &create_request.schema_id,
                create_request.public_key.to_str(),
                create_request.governance_id.to_str(),
                governance_version,
            )?;
            // Una vez que todo va bien creamos el evento prevalidado y lo mandamos a validación
            let event = Signed::<Event>::from_genesis_request(
                event_request.clone(),
                &subject_keys,
                governance_version,
                &initial_state,
            )
            .map_err(EventError::SubjectError)?;
            let event_hash = DigestIdentifier::from_serializable_borsh(&event.content)
                .map_err(|_| EventError::HashGenerationFailed)?;
            let notary_event = self.create_notary_event_from_genesis(
                create_request.clone(),
                event_hash.clone(),
                governance_version,
                subject_id.clone(),
                &subject_keys,
            )?;
            let metadata = notary_event.proof.get_metadata();
            let event_message = create_validator_request(notary_event.clone());
            let stage = ValidationStage::Validate;
            let (signers, quorum_size) = if &create_request.schema_id != "governance" {
                self.get_signers_and_quorum(metadata, stage.clone()).await?
            } else {
                let mut hs = HashSet::new();
                hs.insert(self.own_identifier.clone());
                (hs, 1)
            };
            self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
                .await?;
            self.event_notary_events
                .insert(event_hash.clone(), notary_event);
            // Hacer update de fase por la que va el evento
            self.events_to_validate.insert(event_hash, event.clone());
            self.database
                .set_taple_request(&request_id, &event_request.clone().try_into()?)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            self.database
                .set_request(&subject_id, event_request)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            self.subjects_completing_event
                .insert(subject_id, (stage, signers, (quorum_size, 0)));
            return Ok(request_id);
        }
        let subject_id = match &event_request.content {
            EventRequest::Transfer(tr) => {
                log::warn!("Processing transfer event");
                tr.subject_id.clone()
            }
            EventRequest::EOL(eolr) => {
                log::warn!("Processing EOL event");
                eolr.subject_id.clone()
            }
            EventRequest::Fact(sr) => {
                log::warn!("Processing state event");
                sr.subject_id.clone()
            }
            _ => unreachable!(),
        };
        // Comprobamos si tenemos el sujeto
        let subject = match self.database.get_subject(&subject_id) {
            Ok(subject) => subject,
            Err(error) => match error {
                crate::DbError::EntryNotFound => {
                    return Err(EventError::SubjectNotFound(subject_id.to_str()))
                }
                _ => return Err(EventError::DatabaseError(error.to_string())),
            },
        };
        // Check if we already have an event for that subject
        let None = self.subjects_completing_event.get(&subject.subject_id) else {
            return Err(EventError::EventAlreadyInProgress);
        };
        // Check is subject life has not come to an end
        if !subject.active {
            return Err(EventError::SubjectLifeEnd(subject_id.to_str()));
        }
        log::info!("Subject: {:?}", subject);
        // Chek if we are owner of Subject
        if subject.keys.is_none() {
            return Err(EventError::SubjectNotOwned(subject_id.to_str()));
        }
        // Obtenemos versión actual de la gobernanza
        let gov_version = self
            .gov_api
            .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
            .await
            .map_err(EventError::GovernanceError)?;
        match &event_request.content {
            EventRequest::Transfer(_) | EventRequest::EOL(_) => {
                // TRANSFER
                // Debemos eliminar el material criptográfico del sujeto actual y cambiar su clave pública
                // No obstante, el evento debe firmarse con la clave actual, por lo que no se puede eliminar
                // de inmediato. Debe ser eliminado pues, después de la validación.
                // Estos eventos ni se evaluan, ni se aprueban.
                // No es necesario comprobar la governance, pues no se requieren permisos para la transferencia
                self.process_transfer_or_eol_event(
                    event_request.clone(),
                    subject.clone(),
                    gov_version,
                )
                .await?;
            }
            EventRequest::Fact(state_request) => {
                // Request evaluation signatures, sending request, sn and signature of everything about the subject
                // Get the list of evaluators
                let (metadata, stage) = (
                    Metadata {
                        namespace: subject.namespace,
                        subject_id: subject_id.clone(),
                        governance_id: subject.governance_id.clone(),
                        governance_version: gov_version,
                        schema_id: subject.schema_id,
                    },
                    ValidationStage::Evaluate,
                );
                // Check the invoker can Invoke for this subject
                if event_request.signature.signer != self.own_identifier
                    && !self
                        .gov_api
                        .get_invoke_info(
                            metadata.clone(),
                            ValidationStage::Invoke,
                            event_request.signature.signer.clone(),
                        )
                        .await
                        .map_err(EventError::GovernanceError)?
                {
                    return Err(EventError::InvokePermissionDenied(
                        event_request.signature.signer.to_str(),
                        subject_id.to_str(),
                    ));
                };
                let event_preevaluation = EvaluationRequest {
                    event_request: event_request.clone(),
                    context: SubjectContext {
                        governance_id: metadata.governance_id.clone(),
                        schema_id: metadata.schema_id.clone(),
                        is_owner: subject.owner == event_request.signature.signer,
                        state: subject.properties,
                        // serde_json::to_string(self.virtual_state.get(&subject_id).unwrap())
                        //     .map_err(|_| EventError::ErrorParsingValue)?, // Must be Some, filled in init function
                        namespace: metadata.namespace.clone(),
                    },
                    gov_version,
                    sn: subject.sn + 1,
                    // self.actual_sn.get(&subject_id).unwrap().to_owned() + 1, // Must be Some, filled in init function
                };
                let (signers, quorum_size) =
                    self.get_signers_and_quorum(metadata, stage.clone()).await?;
                log::warn!(
                    "{} PIDIENDO FIRMAS DE EVALUACIÓN {} PARA: {}",
                    subject.sn + 1,
                    quorum_size,
                    subject.subject_id.to_str()
                );
                log::warn!("SIGNERS::::");
                for signer in signers.iter() {
                    log::warn!("{}", signer.to_str());
                }
                let event_preevaluation_hash = DigestIdentifier::from_serializable_borsh(
                    &event_preevaluation,
                )
                .map_err(|_| {
                    EventError::CryptoError(String::from(
                        "Error calculating the hash of the event pre-evaluation",
                    ))
                })?;
                // let er_hash = event_request.signature.content.event_content_hash.clone();
                self.event_pre_evaluations
                    .insert(event_preevaluation_hash, event_preevaluation.clone());
                // if let Some(sn) = self.actual_sn.get_mut(&subject_id) {
                //     *sn += 1;
                // } else {
                //     unreachable!("Unwraped before")
                // }
                // Add the event to the hashset to not complete two at the same time for the same subject
                let negative_quorum_size = (signers.len() as u32 - quorum_size) + 1;
                self.subjects_completing_event.insert(
                    subject_id.clone(),
                    (stage, signers.clone(), (quorum_size, negative_quorum_size)),
                );
                self.ask_signatures(
                    &subject_id,
                    create_evaluator_request(event_preevaluation.clone()),
                    signers.clone(),
                    quorum_size,
                )
                .await?;
            }
            EventRequest::Create(_) => unreachable!(),
        }
        self.subjects_by_governance
            .entry(subject.governance_id)
            .or_insert_with(HashSet::new)
            .insert(subject_id.clone());
        let mut request_data: TapleRequest = event_request.clone().try_into()?;
        request_data.sn = Some(subject.sn + 1);
        request_data.subject_id = Some(subject.subject_id.clone());
        self.database
            .set_taple_request(&request_id, &request_data)
            .map_err(|error| EventError::DatabaseError(error.to_string()))?;
        self.database
            .set_request(&subject.subject_id, event_request)
            .map_err(|error| EventError::DatabaseError(error.to_string()))?;
        Ok(request_id)
    }

    pub async fn evaluator_signatures(
        &mut self,
        evaluation: Evaluation,
        json_patch: ValueWrapper,
        signature: Signature,
    ) -> Result<(), EventError> {
        // Comprobar que el hash devuelto coincide con el hash de la preevaluación
        let preevaluation_event = match self
            .event_pre_evaluations
            .get(&evaluation.preevaluation_hash)
        {
            Some(preevaluation_event) => preevaluation_event,
            None => return Err(EventError::CryptoError(String::from(
                "The hash of the event pre-evaluation does not match any of the pre-evaluations",
            ))),
        };

        let subject_id = match &preevaluation_event.event_request.content {
            // La transferencia no se evalua
            EventRequest::Transfer(_) => return Err(EventError::NoEvaluationForTransferEvents),
            EventRequest::EOL(_) => return Err(EventError::NoEvaluationForEOLEvents),
            EventRequest::Create(_) => {
                return Err(EventError::EvaluationOrApprovationInCreationEvent)
            } // Que hago aquí?? devuelvo error?
            EventRequest::Fact(state_request) => state_request.subject_id.clone(),
        };
        // Mirar en que estado está el evento, si está en evaluación o no
        let Some((ValidationStage::Evaluate, signers, quorum_size)) = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::WrongEventPhase);
        };
        let signer = signature.signer.clone();
        // Check if evaluator is in the list of evaluators
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of evaluators or we already have the signature",
            )));
        }
        // Comprobar que todo es correcto criptográficamente
        signature
            .verify(&evaluation)
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
        let evaluation_hash =
            DigestIdentifier::from_serializable_borsh(&evaluation).map_err(|_| {
                EventError::CryptoError(String::from(
                    "Error calculating the hash of the evaluation",
                ))
            })?;
        // Obtener sujeto para saber si lo tenemos y los metadatos del mismo
        let subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => EventError::SubjectNotFound(subject_id.to_str()),
                _ => EventError::DatabaseError(error.to_string()),
            })?;
        // Comprobar si la versión de la governanza coincide con la nuestra, si no no lo aceptamos
        let governance_version = self
            .gov_api
            .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
            .await
            .map_err(EventError::GovernanceError)?;
        // Comprobar governance-version que sea la misma que la nuestra
        if governance_version != evaluation.governance_version {
            return Err(EventError::WrongGovernanceVersion);
        }
        // Comprobar que el json patch es válido
        if !hash_match_after_patch(&evaluation, json_patch.clone(), subject.properties.clone())? {
            return Err(EventError::CryptoError(
                "Json patch applied to state hash does not match the new state hash".to_string(),
            ));
        }
        // Guardar evaluación
        let signatures_set = match self
            .event_evaluations
            .get_mut(&evaluation.preevaluation_hash)
        {
            Some(signatures_set) => {
                insert_or_replace_and_check(
                    signatures_set,
                    (
                        UniqueSignature { signature },
                        evaluation.acceptance.clone(),
                        evaluation_hash.clone(),
                    ),
                );
                signatures_set
            }
            None => {
                let mut new_signatures_set = HashSet::new();
                new_signatures_set.insert((
                    UniqueSignature { signature },
                    evaluation.acceptance.clone(),
                    evaluation_hash.clone(),
                ));
                self.event_evaluations
                    .insert(evaluation.preevaluation_hash.clone(), new_signatures_set);
                self.event_evaluations
            .get_mut(&evaluation.preevaluation_hash)
            .expect("Acabamos de insertar el conjunto de firmas, por lo que debe estar presente")
            }
        };
        let (num_signatures_hash_ok, num_signatures_hash_ko) =
            count_signatures_with_event_content_hash(&signatures_set, &evaluation_hash);
        let (quorum_size, negative_quorum_size) = quorum_size.to_owned();
        // Comprobar si llegamos a Quorum
        let quorum_reached = {
            if num_signatures_hash_ok >= quorum_size {
                Some(Acceptance::Ok)
            } else if num_signatures_hash_ko >= negative_quorum_size {
                Some(Acceptance::Ko)
            } else {
                None
            }
        };
        if quorum_reached.is_none() {
            log::error!("SE EJECUTA IS NONE");
            let mut new_signers: HashSet<KeyIdentifier> =
                signers.into_iter().map(|s| s.clone()).collect();
            new_signers.remove(&signer);
            self.ask_signatures(
                &subject_id,
                create_evaluator_request(preevaluation_event.clone()),
                new_signers.clone(),
                quorum_size,
            )
            .await?;
            // Hacer update de fase por la que va el evento
            self.subjects_completing_event.insert(
                subject_id,
                (
                    ValidationStage::Evaluate,
                    new_signers,
                    (quorum_size, negative_quorum_size),
                ),
            );
            return Ok(()); // No llegamos a quorum, no hacemos nada
        } else {
            log::error!("LLEGA A QUORUM");
            // Si es así comprobar que json patch aplicado al evento parar la petición de firmas y empezar a pedir las approves con el evento completo con lo nuevo obtenido en esta fase si se requieren approves, si no informar a validator
            // Comprobar que al aplicar Json Patch llegamos al estado final?
            // Crear Event Proposal
            let evaluator_signatures = signatures_set
                .iter()
                .filter(|(signature, acceptance, hash)| {
                    hash == &evaluation_hash && quorum_reached.as_ref().unwrap() == acceptance
                })
                .map(|(signature, _, _)| signature.signature.clone())
                .collect();
            let hash_prev_event = DigestIdentifier::from_serializable_borsh(
                &self
                    .database
                    .get_event(&subject.subject_id, subject.sn)
                    .map_err(|e| EventError::DatabaseError(e.to_string()))?
                    .content,
            )
            .map_err(|_| {
                EventError::CryptoError(
                    "Error calculating the hash of the previous event".to_owned(),
                )
            })?;
            let proposal = ApprovalRequest::new(
                preevaluation_event.event_request.clone(),
                preevaluation_event.sn,
                hash_prev_event,
                evaluation.governance_version,
                Some(evaluation.clone()),
                json_patch,
                evaluator_signatures,
            );
            let proposal_hash =
                DigestIdentifier::from_serializable_borsh(&proposal).map_err(|_| {
                    EventError::CryptoError(String::from(
                        "Error calculating the hash of the proposal",
                    ))
                })?;
            let subject_keys = subject
                .keys
                .clone()
                .expect("Llegados a aquí tenemos que ser owner");
            let subject_signature =
                Signature::new(&proposal, subject.public_key.clone(), &subject_keys).map_err(
                    |_| EventError::CryptoError(String::from("Error signing the proposal")),
                )?;
            let event_proposal = Signed::<ApprovalRequest>::new(proposal, subject_signature);
            let metadata = Metadata {
                namespace: subject.namespace.clone(),
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id.clone(),
                governance_version,
                schema_id: subject.schema_id.clone(),
            };
            // Añadir al hashmap para poder acceder a él cuando lleguen las firmas de los evaluadores
            self.event_proposals
                .insert(proposal_hash, event_proposal.clone());
            // Pedir Approves si es necesario, si no pedir validaciones
            let (stage, event_message) = if evaluation.approval_required {
                log::error!("SE PIDEN APROBACIONES");
                let msg = create_approval_request(event_proposal);
                // Retornar TapleMessage directamente
                (ValidationStage::Approve, msg)
            } else {
                // No se necesita aprobación
                let execution = match evaluation.acceptance {
                    crate::commons::models::Acceptance::Ok => true,
                    crate::commons::models::Acceptance::Ko => false,
                };
                let gov_version = self
                    .gov_api
                    .get_governance_version(
                        subject.governance_id.clone(),
                        subject.subject_id.clone(),
                    )
                    .await?;
                let event = &self.create_event_prevalidated(
                    event_proposal,
                    HashSet::new(),
                    &subject,
                    execution,
                )?;
                let event_hash = DigestIdentifier::from_serializable_borsh(&event.content)
                    .map_err(|_| {
                        EventError::CryptoError("Error generating event hash".to_owned())
                    })?;
                let notary_event = self.create_notary_event(&subject, &event, gov_version)?;
                let event_message = create_validator_request(notary_event.clone());
                self.event_notary_events.insert(event_hash, notary_event);
                (ValidationStage::Validate, event_message)
            };
            // Limpiar HashMaps
            self.event_evaluations
                .remove(&evaluation.preevaluation_hash);
            self.event_pre_evaluations
                .remove(&evaluation.preevaluation_hash);
            let (signers, quorum_size) =
                self.get_signers_and_quorum(metadata, stage.clone()).await?;
            self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
                .await?;
            // Hacer update de fase por la que va el evento
            log::error!("LLEGA A ACTUALIZAR EL STAGE QUE ES: {:?}", stage);
            log::error!(
                "LLEGA A ACTUALIZAR EL STAGE PARA EL SUJETO: {:?}",
                subject_id.to_str()
            );
            let negative_quorum_size = (signers.len() as u32 - quorum_size) + 1;
            self.subjects_completing_event.insert(
                subject_id.clone(),
                (stage, signers, (quorum_size, negative_quorum_size)),
            );
            let tmp = self.subjects_completing_event.get(&subject_id);
        }
        Ok(())
    }

    pub async fn approver_signatures(
        &mut self,
        approval: Signed<ApprovalResponse>,
    ) -> Result<(), EventError> {
        log::warn!("APPROVAL SIGNATURES");
        log::warn!("APPROVAL 1");
        // Mirar en que estado está el evento, si está en aprovación o no
        let event_proposal = match self
            .event_proposals
            .get(&approval.content.appr_req_hash)
        {
            Some(event_proposal) => event_proposal,
            None => {
                return Err(EventError::CryptoError(String::from(
                    "The hash of the event proposal does not match any of the proposals",
                )))
            }
        };
        log::warn!("APPROVAL 2");
        let subject_id = match &event_proposal.content.event_request.content {
            // La transferencia no se aprueba
            EventRequest::Transfer(_) => return Err(EventError::NoAprovalForTransferEvents),
            // EOL no se aprueba
            EventRequest::EOL(_) => return Err(EventError::NoAprovalForEOLEvents),
            EventRequest::Create(_) => {
                return Err(EventError::EvaluationOrApprovationInCreationEvent)
            } // Que hago aquí?? devuelvo error?
            EventRequest::Fact(state_request) => state_request.subject_id.clone(),
        };
        log::warn!("APPROVAL 3");
        let tmp = self.subjects_completing_event.get(&subject_id);
        log::error!("STAGE: {:?}", tmp);
        let Some((ValidationStage::Approve, signers, quorum_size)) = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::WrongEventPhase);
        };
        log::warn!("APPROVAL 4");
        let signer = approval.signature.signer.clone();
        // Check if approver is in the list of approvers
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of approvers or we already have his approve",
            )));
        }
        log::warn!("APPROVAL 5");
        // Comprobar que todo es correcto criptográficamente
        approval
            .verify()
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
        log::warn!("APPROVAL 6");
        // Obtener sujeto para saber si lo tenemos y los metadatos del mismo
        let subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => EventError::SubjectNotFound(subject_id.to_str()),
                _ => EventError::DatabaseError(error.to_string()),
            })?;
        log::warn!("APPROVAL 7");
        // Guardar aprobación
        let approval_set = match self
            .event_approvations
            .get_mut(&approval.content.appr_req_hash)
        {
            Some(approval_set) => {
                insert_or_replace_and_check(
                    approval_set,
                    UniqueApproval {
                        approval: approval.clone(),
                    },
                );
                approval_set
            }
            None => {
                let mut new_approval_set = HashSet::new();
                new_approval_set.insert(UniqueApproval {
                    approval: approval.clone(),
                });
                self.event_approvations.insert(
                    approval.content.appr_req_hash.clone(),
                    new_approval_set,
                );
                self.event_approvations
            .get_mut(&approval.content.appr_req_hash)
            .expect("Acabamos de insertar el conjunto de approvals, por lo que debe estar presente")
            }
        };
        // Comprobar si llegamos a Quorum positivo o negativo
        let num_approvals_with_same_acceptance = approval_set
            .iter()
            .filter(|unique_approval| {
                unique_approval.approval.content.approved == approval.content.approved
            })
            .count() as u32;
        let (quorum_size_now, execution) = match approval.content.approved {
            crate::commons::models::Acceptance::Ok => (quorum_size.0, true),
            crate::commons::models::Acceptance::Ko => (quorum_size.1, false),
        };
        if num_approvals_with_same_acceptance < quorum_size_now {
            let mut new_signers: HashSet<KeyIdentifier> =
                signers.into_iter().map(|s| s.clone()).collect();
            new_signers.remove(&signer);
            self.ask_signatures(
                &subject_id,
                create_approval_request(event_proposal.to_owned()),
                signers.clone(),
                quorum_size_now,
            )
            .await?;
            // Hacer update de fase por la que va el evento
            self.subjects_completing_event.insert(
                subject_id,
                (
                    ValidationStage::Approve,
                    new_signers,
                    quorum_size.to_owned(),
                ),
            );
            Ok(()) // No llegamos a quorum, no hacemos nada
        } else {
            let governance_version = self
                .gov_api
                .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
                .await
                .map_err(EventError::GovernanceError)?;
            // Si se llega a Quorum dejamos de pedir approves y empezamos a pedir notarizaciones con el evento completo incluyendo lo nuevo de las approves
            let metadata = Metadata {
                namespace: subject.namespace.clone(),
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id.clone(),
                governance_version,
                schema_id: subject.schema_id.clone(),
            };
            // Creamos el evento final
            let approvals = approval_set
                .iter()
                .filter(|unique_approval| {
                    unique_approval.approval.content.approved == approval.content.approved
                })
                .map(|approval| approval.approval.clone())
                .collect();
            let event_proposal = self
                .event_proposals
                .remove(&approval.content.appr_req_hash)
                .unwrap();

            let gov_version = self
                .gov_api
                .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
                .await?;
            let event =
                &self.create_event_prevalidated(event_proposal, approvals, &subject, execution)?;
            let notary_event = self.create_notary_event(&subject, &event, gov_version)?;
            let event_message = create_validator_request(notary_event.clone());
            let event_hash = DigestIdentifier::from_serializable_borsh(&event.content)
                .map_err(|_| EventError::CryptoError("Error generating event hash".to_owned()))?;
            // Limpiar HashMaps
            self.event_approvations
                .remove(&approval.content.appr_req_hash);
            let stage = ValidationStage::Validate;
            let (signers, quorum_size) =
                self.get_signers_and_quorum(metadata, stage.clone()).await?;
            self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
                .await?;
            self.event_notary_events.insert(event_hash, notary_event);
            // Hacer update de fase por la que va el evento
            self.subjects_completing_event
                .insert(subject_id, (stage, signers, (quorum_size, 0)));
            Ok(())
        }
    }

    pub async fn validation_signatures(
        &mut self,
        event_hash: DigestIdentifier,
        signature: Signature,
        governance_version: u64,
    ) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en notarización o no
        let event = match self.events_to_validate.get(&event_hash) {
            Some(event) => event,
            None => {
                log::warn!("SE EJECUTA NONE");
                return Err(EventError::CryptoError(String::from(
                    "The hash of the event does not match any of the events",
                )));
            }
        };
        log::warn!("VALIDATION AFTER EVENT");
        let notary_event = self
            .event_notary_events
            .get(&event_hash)
            .expect("Should be");
        log::warn!("VALIDATION AFTER EXPECT");
        let subject_id = match &event.content.event_proposal.content.event_request.content {
            EventRequest::Transfer(transfer_request) => transfer_request.subject_id.clone(),
            EventRequest::EOL(eol_request) => eol_request.subject_id.clone(),
            EventRequest::Create(create_request) => generate_subject_id(
                &create_request.namespace,
                &create_request.schema_id,
                create_request.public_key.to_str(),
                create_request.governance_id.to_str(),
                event.content.event_proposal.content.gov_version,
            )?, // Que hago aquí?? devuelvo error?
            EventRequest::Fact(state_request) => state_request.subject_id.clone(),
        };
        // Obtener sujeto para saber si lo tenemos y los metadatos del mismo
        let subject = match self.database.get_subject(&subject_id) {
            Ok(subject) => Some(subject),
            Err(error) => match error {
                crate::DbError::EntryNotFound => None,
                _ => return Err(EventError::DatabaseError(error.to_string())),
            },
        };
        log::warn!("PASO 1");
        let (our_governance_version, governance_id) =
            if event.content.event_proposal.content.sn == 0 && subject.is_none() {
                if let EventRequest::Create(create_request) =
                    &event.content.event_proposal.content.event_request.content
                {
                    if create_request.schema_id == "governance" {
                        (0, create_request.governance_id.clone())
                    } else {
                        (
                            self.gov_api
                                .get_governance_version(
                                    create_request.governance_id.clone(),
                                    subject_id.clone(),
                                )
                                .await
                                .map_err(EventError::GovernanceError)?,
                            create_request.governance_id.clone(),
                        )
                    }
                } else {
                    return Err(EventError::Event0NotCreate);
                }
            } else if subject.is_some() && event.content.event_proposal.content.sn != 0 {
                let subject = subject.unwrap();
                if subject.schema_id == "governance" {
                    (subject.sn, subject.subject_id.clone())
                } else {
                    (
                        self.gov_api
                            .get_governance_version(
                                subject.governance_id.clone(),
                                subject.subject_id.clone(),
                            )
                            .await
                            .map_err(EventError::GovernanceError)?,
                        subject.governance_id,
                    )
                }
            } else {
                return Err(EventError::SubjectNotFound(subject_id.to_str()));
            };
        if our_governance_version < governance_version {
            // Ignoramos la firma de validación porque no nos vale, pero pedimos la governance al validador que nos la ha enviado
            let msg = request_gov_event(
                self.own_identifier.clone(),
                governance_id,
                our_governance_version + 1,
            );
            self.message_channel
                .tell(MessageTaskCommand::Request(
                    None,
                    msg,
                    vec![signature.signer],
                    MessageConfig {
                        timeout: 2000,
                        replication_factor: 1.0,
                    },
                ))
                .await?;
            return Ok(());
        } else if our_governance_version > governance_version {
            // Ignoramos la firma de validación porque no nos vale
            return Ok(());
        }
        log::warn!("TDO BIEN");
        let a = self.subjects_completing_event.get(&subject_id);
        log::warn!("STAGE: {:?}", a);
        // CHeck phase
        let Some((ValidationStage::Validate, signers, quorum_size)) = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::WrongEventPhase);
        };
        log::warn!("PASO 2");
        let signer = signature.signer.clone();
        // Check if approver is in the list of approvers
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of validators or we already have the validation",
            )));
        }
        log::warn!("PASO 3");
        // Comprobar que todo es correcto criptográficamente
        let event_hash = DigestIdentifier::from_serializable_borsh(&notary_event.proof)
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
        signature
            .verify(&notary_event.proof)
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
        log::warn!("PASO 4");
        // Guardar validación
        let validation_set = match self.event_validations.get_mut(&event_hash) {
            Some(validation_set) => {
                insert_or_replace_and_check(validation_set, UniqueSignature { signature });
                validation_set
            }
            None => {
                let mut new_validation_set = HashSet::new();
                new_validation_set.insert(UniqueSignature { signature });
                self.event_validations
                    .insert(event_hash.clone(), new_validation_set);
                self.event_validations.get_mut(&event_hash).expect(
                    "Acabamos de insertar el conjunto de validations, por lo que debe estar presente",
                )
            }
        };
        log::warn!("PASO 5");
        let quorum_size = quorum_size.to_owned();
        // Comprobar si llegamos a Quorum y si es así dejar de pedir firmas
        if (validation_set.len() as u32) < quorum_size.0 {
            log::warn!("PASO 6 IF");
            let notary_event = match self.event_notary_events.get(&event_hash) {
                Some(notary_event) => notary_event.to_owned(),
                None => {
                    return Err(EventError::CryptoError(String::from(
                        "The hash of the event does not match any of the events",
                    )))
                }
            };
            let event_message = create_validator_request(notary_event);
            let mut new_signers: HashSet<KeyIdentifier> =
                signers.into_iter().map(|s| s.clone()).collect();
            new_signers.remove(&signer);
            self.ask_signatures(
                &subject_id,
                event_message,
                new_signers.clone(),
                quorum_size.0,
            )
            .await?;
            // Hacer update de fase por la que va el evento
            self.subjects_completing_event.insert(
                subject_id,
                (ValidationStage::Validate, new_signers, quorum_size),
            );
            Ok(())
        } else {
            log::warn!("PASO 6 ELSE");
            let validation_signatures: HashSet<Signature> = validation_set
                .iter()
                .map(|unique_signature| unique_signature.signature.clone())
                .collect();
            // Si se llega a Quorum lo mandamos al ledger
            if event.content.event_proposal.content.sn == 0 {
                self.ledger_sender
                    .tell(LedgerCommand::Genesis {
                        event: event.clone(),
                        signatures: validation_signatures,
                        validation_proof: notary_event.proof.clone(),
                    })
                    .await?;
            } else {
                self.ledger_sender
                    .tell(LedgerCommand::OwnEvent {
                        event: event.clone(),
                        signatures: validation_signatures,
                        validation_proof: notary_event.proof.clone(),
                    })
                    .await?;
            }
            self.database
                .del_prevalidated_event(&subject_id)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            // Por si es Create, Florentín dice que no da error si no existe lo que hay que borrar
            self.database
                .del_request(&subject_id)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            // Cancelar pedir firmas
            self.message_channel
                .tell(MessageTaskCommand::Cancel(String::from(format!(
                    "{}",
                    subject_id.to_str()
                ))))
                .await
                .map_err(EventError::ChannelError)?;
            // Limpiar HashMaps
            self.events_to_validate.remove(&event_hash);
            self.event_validations.remove(&event_hash);
            self.subjects_completing_event.remove(&subject_id);
            self.subjects_by_governance.remove(&subject_id);
            Ok(())
        }
    }

    pub async fn higher_governance_expected(
        &self,
        governance_id: DigestIdentifier,
        who_asked: KeyIdentifier,
    ) -> Result<(), EventError> {
        self.message_channel
            .tell(MessageTaskCommand::Request(
                None,
                TapleMessages::LedgerMessages(LedgerCommand::GetLCE {
                    who_asked: self.own_identifier.clone(),
                    subject_id: governance_id,
                }),
                vec![who_asked],
                MessageConfig {
                    timeout: TIMEOUT,
                    replication_factor: 1.0,
                },
            ))
            .await
            .map_err(EventError::ChannelError)
    }

    async fn get_signers_and_quorum(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<(HashSet<KeyIdentifier>, u32), EventError> {
        let signers = self
            .gov_api
            .get_signers(metadata.clone(), stage.clone())
            .await
            .map_err(EventError::GovernanceError)?;
        let quorum_size = self
            .gov_api
            .get_quorum(metadata, stage)
            .await
            .map_err(EventError::GovernanceError)?;
        Ok((signers, quorum_size))
    }

    async fn ask_signatures(
        &self,
        subject_id: &DigestIdentifier,
        event_message: TapleMessages,
        signers: HashSet<KeyIdentifier>,
        quorum_size: u32,
    ) -> Result<(), EventError> {
        let replication_factor = extend_quorum(quorum_size, signers.len());
        self.message_channel
            .tell(MessageTaskCommand::Request(
                Some(String::from(format!("{}", subject_id.to_str()))),
                event_message,
                signers.into_iter().collect(),
                MessageConfig {
                    timeout: TIMEOUT,
                    replication_factor,
                },
            ))
            .await
            .map_err(EventError::ChannelError)?;
        Ok(())
    }

    fn create_event_prevalidated(
        &mut self,
        event_proposal: Signed<ApprovalRequest>,
        approvals: HashSet<Signed<ApprovalResponse>>,
        subject: &Subject,
        execution: bool,
    ) -> Result<Signed<Event>, EventError> {
        let event_content = Event::new(event_proposal, approvals, execution);
        let event_content_hash = DigestIdentifier::from_serializable_borsh(&event_content)
            .map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the event"))
            })?;
        let subject_keys = subject.keys.as_ref().expect("Somos propietario");
        let event_signature = Signature::new(
            &event_content,
            subject.public_key.clone(),
            &subject_keys,
        )
        .map_err(|_| {
            EventError::CryptoError(String::from("Error signing the hash of the event content"))
        })?;
        let event = Signed::<Event> {
            content: event_content,
            signature: event_signature,
        };
        self.events_to_validate
            .insert(event_content_hash, event.clone());
        self.database
            .set_prevalidated_event(&subject.subject_id, event.clone())
            .map_err(|error| EventError::DatabaseError(error.to_string()))?;
        self.database
            .del_request(&subject.subject_id)
            .map_err(|error| EventError::DatabaseError(error.to_string()))?;
        Ok(event)
    }
}

pub fn extend_quorum(quorum_size: u32, signers_len: usize) -> f64 {
    let quorum_extended =
        quorum_size + (signers_len as f64 * QUORUM_PORCENTAGE_AMPLIFICATION).ceil() as u32;
    quorum_extended as f64 / signers_len as f64
}

fn count_signatures_with_event_content_hash(
    signatures: &HashSet<(UniqueSignature, Acceptance, DigestIdentifier)>,
    target_event_content_hash: &DigestIdentifier,
) -> (u32, u32) {
    let mut ok: u32 = 0;
    let mut ko: u32 = 0;
    for (signature, acceptance, hash) in signatures.iter() {
        if hash == target_event_content_hash {
            match acceptance {
                Acceptance::Ok => ok += 1,
                Acceptance::Ko => ko += 1,
            }
        }
    }
    (ok, ko)
}

fn insert_or_replace_and_check<T: PartialEq + Eq + Hash>(
    set: &mut HashSet<T>,
    new_value: T,
) -> bool {
    let replaced = set.remove(&new_value); // Si existe un valor igual, lo eliminamos y devolvemos true.
    set.insert(new_value); // Insertamos el nuevo valor.
    replaced // Devolvemos si se ha reemplazado un elemento existente.
}

fn hash_match_after_patch(
    evaluation: &Evaluation,
    json_patch: ValueWrapper,
    mut prev_properties: ValueWrapper,
) -> Result<bool, EventError> {
    if evaluation.acceptance != Acceptance::Ok {
        let state_hash_calculated = DigestIdentifier::from_serializable_borsh(&prev_properties)
            .map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the state"))
            })?;
        Ok(state_hash_calculated == evaluation.state_hash)
    } else {
        let Ok(patch_json) = serde_json::from_value::<Patch>(json_patch.0) else {
            return Err(EventError::ErrorParsingJsonString("Error Parsing Patch".to_owned()));
    };
        let Ok(()) = patch(&mut prev_properties.0, &patch_json) else {
        return Err(EventError::ErrorApplyingPatch("Error applying patch".to_owned()));
    };
        let state_hash_calculated = DigestIdentifier::from_serializable_borsh(&prev_properties)
            .map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the state"))
            })?;
        Ok(state_hash_calculated == evaluation.state_hash)
    }
}
