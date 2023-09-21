use std::collections::{HashMap, HashSet};

use json_patch::{patch, Patch};

use crate::{
    commons::{
        channel::SenderEnd,
        models::{
            approval::UniqueApproval,
            evaluation::{EvaluationRequest, SubjectContext},
            event::Event,
            event::Metadata,
            state::{generate_subject_id, Subject},
            validation::ValidationProof,
            HashId,
        },
        self_signature_manager::SelfSignatureManager,
    },
    crypto::KeyPair,
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    ledger::{LedgerCommand, LedgerResponse},
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
    request::StartRequest,
    request::TapleRequest,
    signature::{Signature, Signed, UniqueSignature},
    utils::message::{
        approval::create_approval_request, evaluator::create_evaluator_request,
        ledger::request_gov_event, validation::create_validator_request,
    },
    validation::ValidationEvent,
    ApprovalRequest, ApprovalResponse, DatabaseCollection, EvaluationResponse, EventRequest,
    Notification, ValueWrapper,
};
use std::hash::Hash;

use super::errors::EventError;
use crate::database::DB;

const TIMEOUT: u32 = 2000;
// const GET_ALL: isize = 200;
const QUORUM_PORCENTAGE_AMPLIFICATION: f64 = 0.2;

#[allow(dead_code)]
pub struct EventCompleter<C: DatabaseCollection> {
    gov_api: GovernanceAPI,
    database: DB<C>,
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    notification_tx: tokio::sync::mpsc::Sender<Notification>,
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
        HashMap<DigestIdentifier, HashSet<(UniqueSignature, bool, DigestIdentifier)>>,
    // Approval HashMaps
    approval_eval_signatures: HashMap<DigestIdentifier, HashSet<Signature>>,
    approval_requests: HashMap<DigestIdentifier, Signed<ApprovalRequest>>,
    event_approvations: HashMap<DigestIdentifier, HashSet<UniqueApproval>>,
    // Validation HashMaps
    events_to_validate: HashMap<DigestIdentifier, Signed<Event>>,
    event_validations: HashMap<DigestIdentifier, HashSet<UniqueSignature>>,
    event_validation_events: HashMap<DigestIdentifier, ValidationEvent>,
    // SignatureManager
    signature_manager: SelfSignatureManager,
}

#[allow(dead_code)]
impl<C: DatabaseCollection> EventCompleter<C> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<C>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        notification_tx: tokio::sync::mpsc::Sender<Notification>,
        ledger_sender: SenderEnd<LedgerCommand, LedgerResponse>,
        own_identifier: KeyIdentifier,
        signature_manager: SelfSignatureManager,
    ) -> Self {
        Self {
            gov_api,
            database,
            message_channel,
            notification_tx,
            ledger_sender,
            subjects_completing_event: HashMap::new(),
            // actual_sn: HashMap::new(),
            // virtual_state: HashMap::new(),
            event_pre_evaluations: HashMap::new(),
            event_evaluations: HashMap::new(),
            approval_eval_signatures: HashMap::new(),
            approval_requests: HashMap::new(),
            events_to_validate: HashMap::new(),
            event_approvations: HashMap::new(),
            event_validations: HashMap::new(),
            subjects_by_governance: HashMap::new(),
            event_validation_events: HashMap::new(),
            own_identifier,
            signature_manager,
        }
    }

    fn create_validation_event_from_genesis(
        &self,
        create_request: StartRequest,
        event_hash: DigestIdentifier,
        governance_version: u64,
        subject_id: DigestIdentifier,
        subject_keys: &KeyPair,
    ) -> Result<ValidationEvent, EventError> {
        let validation_proof = ValidationProof::new_from_genesis_event(
            create_request,
            event_hash,
            governance_version,
            subject_id,
        );
        let subject_signature = Signature::new(&validation_proof, subject_keys)?;
        Ok(ValidationEvent {
            proof: validation_proof,
            subject_signature,
            previous_proof: None,
            prev_event_validation_signatures: HashSet::new(),
        })
    }

    fn create_validation_event(
        &self,
        subject: &Subject,
        event: &Signed<Event>,
        gov_version: u64,
    ) -> Result<ValidationEvent, EventError> {
        let proof = match &event.content.event_request.content {
            EventRequest::Create(_) | EventRequest::Fact(_) | EventRequest::EOL(_) => {
                ValidationProof::new(
                    subject,
                    event.content.sn,
                    event.content.hash_prev_event.clone(),
                    event.content.hash_id()?,
                    gov_version,
                )
            }
            EventRequest::Transfer(transfer_request) => ValidationProof::new_from_transfer_event(
                subject,
                event.content.sn,
                event.content.hash_prev_event.clone(),
                event.content.hash_id()?,
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
                let subject_signature = Signature::new(&proof, keys)?;
                Ok(ValidationEvent {
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
            if subject.schema_id != "governance" {
                self.subjects_by_governance
                    .entry(subject.governance_id.clone())
                    .or_insert_with(HashSet::new)
                    .insert(subject.subject_id.clone());
            }
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
                    let validation_event =
                        self.create_validation_event(subject, &last_event, gov_version)?;
                    let event_message = create_validator_request(validation_event.clone());
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
                    self.event_validation_events
                        .insert(last_event_hash.clone(), validation_event);
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
            // Check if there are requests in the database that correspond to events that have not yet reached the validation phase and should be restarted from requesting evaluations.
            match self.database.get_request(&subject.subject_id) {
                Ok(event_request) => {
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
        // Ask for event requests for each subject_id of the set and launch new_event with them
        match self.subjects_by_governance.get(&governance_id).cloned() {
            Some(subjects_affected) => {
                for subject_id in subjects_affected.iter() {
                    match self.database.get_request(subject_id) {
                        Ok(event_request) => {
                            let EventRequest::Fact(_) = &event_request.content else {
                                return Err(EventError::GenesisInGovUpdate);
                            };
                            self.new_event(event_request).await?;
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
                            if let EventRequest::Create(_) =
                                &event_prevalidated.content.event_request.content
                            {
                                // Cancel signature request
                                self.message_channel
                                    .tell(MessageTaskCommand::Cancel(String::from(format!(
                                        "{}",
                                        event_prevalidated.content.subject_id.to_str()
                                    ))))
                                    .await
                                    .map_err(EventError::ChannelError)?;
                                self.subjects_completing_event.remove(&subject_id);
                                self.subjects_by_governance.remove(&subject_id);
                                self.database.del_prevalidated_event(&subject_id).map_err(
                                    |error| EventError::DatabaseError(error.to_string()),
                                )?;
                                self.new_event(event_prevalidated.content.event_request)
                                    .await?;
                                continue;
                            }
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
                            let validation_event = self.create_validation_event(
                                &subject,
                                &event_prevalidated,
                                new_version,
                            )?;
                            let event_message = create_validator_request(validation_event.clone());
                            let stage = ValidationStage::Validate;
                            let (signers, quorum_size) =
                                self.get_signers_and_quorum(metadata, stage.clone()).await?;
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
                            self.event_validation_events
                                .insert(event_prevalidated_hash, validation_event);
                            // Make update of the phase the event is going through
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
        // Add to the hashmap to be able to access it when the validator signatures arrive.
        let event =
            &self.create_event_prevalidated_no_eval(event_request, &subject, gov_version)?;
        let event_hash = DigestIdentifier::from_serializable_borsh(&event.content)
            .map_err(|_| EventError::CryptoError("Error generating event hash".to_owned()))?;
        let validation_event = self.create_validation_event(&subject, &event, gov_version)?;
        let event_message = create_validator_request(validation_event.clone());
        self.event_validation_events
            .insert(event_hash, validation_event);
        let stage = ValidationStage::Validate;
        let (signers, quorum_size) = self.get_signers_and_quorum(metadata, stage.clone()).await?;
        self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
            .await?;
        // Make update of the phase the event is going through
        self.subjects_completing_event
            .insert(subject_id.clone(), (stage, signers, (quorum_size, 0)));
        Ok(())
    }

    async fn generate_event_proposal(
        &self,
        event_request: &Signed<EventRequest>,
        subject: &Subject,
        gov_version: u64,
    ) -> Result<Signed<ApprovalRequest>, EventError> {
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
        let approval_request = ApprovalRequest {
            event_request: event_request.clone(),
            sn: subject.sn + 1,
            gov_version,
            patch: ValueWrapper(
                serde_json::from_str("[]")
                    .map_err(|_| EventError::CryptoError("Error parsing empty json".to_string()))?,
            ),
            state_hash: subject.properties.hash_id()?,
            hash_prev_event,
            gov_id: subject.governance_id.clone(),
        };
        let subject_signature = Signature::new(
            &approval_request,
            &subject
                .keys
                .as_ref()
                .expect("Llegados a aquí tenemos que ser owner"),
        )
        .map_err(|_| {
            EventError::CryptoError(String::from("Error signing the hash of the proposal"))
        })?;
        Ok(Signed::<ApprovalRequest> {
            content: approval_request,
            signature: subject_signature,
        })
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
        let subject_id: &DigestIdentifier = match &event_request.content {
            EventRequest::Create(_) => return self.new_event(event_request).await,
            EventRequest::Fact(fact_req) => &fact_req.subject_id,
            EventRequest::Transfer(trans_req) => &trans_req.subject_id,
            EventRequest::EOL(eol_req) => &eol_req.subject_id,
        };
        // Check if we already have an event for that subject
        let None = self.subjects_completing_event.get(subject_id) else {
            return Err(EventError::EventAlreadyInProgress);
        };
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
            // Check if it is governance, then anything goes, otherwise check that the invoker is me and I can do it.
            if event_request.signature.signer != self.own_identifier {
                return Err(EventError::ExternalGenesisEvent);
            }
            if create_request.public_key.public_key.is_empty() {
                return Err(EventError::PublicKeyIsEmpty);
            }
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
            let (governance_version, initial_state) = if &create_request.schema_id != "governance" {
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
            // Once everything goes well, we create the pre-validated event and send it to validation.
            let event = Signed::<Event>::from_genesis_request(
                event_request.clone(),
                &subject_keys,
                governance_version,
                &initial_state,
            )
            .map_err(EventError::SubjectError)?;
            let event_hash = DigestIdentifier::from_serializable_borsh(&event.content)
                .map_err(|_| EventError::HashGenerationFailed)?;
            let validation_event = self.create_validation_event_from_genesis(
                create_request.clone(),
                event_hash.clone(),
                governance_version,
                subject_id.clone(),
                &subject_keys,
            )?;
            let metadata = validation_event.proof.get_metadata();
            let event_message = create_validator_request(validation_event.clone());
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
            self.event_validation_events
                .insert(event_hash.clone(), validation_event);
            // Make update of the phase the event is going through
            self.events_to_validate.insert(event_hash, event.clone());
            self.database
                .set_taple_request(&request_id, &event_request.clone().try_into()?)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            self.database
                .set_prevalidated_event(&subject_id, event.clone())
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            if create_request.schema_id != "governance" {
                self.subjects_by_governance
                    .entry(create_request.governance_id.clone())
                    .or_insert_with(HashSet::new)
                    .insert(subject_id.clone());
            }
            self.subjects_completing_event
                .insert(subject_id, (stage, signers, (quorum_size, 0)));
            return Ok(request_id);
        }
        let subject_id = match &event_request.content {
            EventRequest::Transfer(tr) => {
                log::info!("Processing transfer event");
                tr.subject_id.clone()
            }
            EventRequest::EOL(eolr) => {
                log::info!("Processing EOL event");
                eolr.subject_id.clone()
            }
            EventRequest::Fact(sr) => {
                log::info!("Processing state event");
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
        // Check is subject life has not come to an end
        if !subject.active {
            return Err(EventError::SubjectLifeEnd(subject_id.to_str()));
        }
        // Chek if we are owner of Subject
        if subject.keys.is_none() {
            return Err(EventError::SubjectNotOwned(subject_id.to_str()));
        }
        // We obtain the current version of governance
        let gov_version = self
            .gov_api
            .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
            .await
            .map_err(EventError::GovernanceError)?;
        match &event_request.content {
            EventRequest::Transfer(_) | EventRequest::EOL(_) => {
                // TRANSFER
                // We must remove the cryptographic material of the current subject and change its public key.
                // However, the event must be signed with the current key, so it cannot be deleted
                // immediately. It must therefore be deleted after validation.
                // These events are neither evaluated nor approved.
                // It is not necessary to check the governance, as no permissions are required for the transfer.
                self.process_transfer_or_eol_event(
                    event_request.clone(),
                    subject.clone(),
                    gov_version,
                )
                .await?;
            }
            EventRequest::Fact(_) => {
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
                // log::info!(
                //     "{} PIDIENDO FIRMAS DE EVALUACIÓN {} PARA: {}",
                //     subject.sn + 1,
                //     quorum_size,
                //     subject.subject_id.to_str()
                // );
                // log::info!("SIGNERS::::");
                // for signer in signers.iter() {
                //     log::warn!("{}", signer.to_str());
                // }
                let event_preevaluation_hash = DigestIdentifier::from_serializable_borsh(
                    &event_preevaluation,
                )
                .map_err(|_| {
                    EventError::CryptoError(String::from(
                        "Error calculating the hash of the event pre-evaluation",
                    ))
                })?;
                self.event_pre_evaluations
                    .insert(event_preevaluation_hash, event_preevaluation.clone());
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
            .entry(subject.governance_id.clone())
            .or_insert_with(HashSet::new)
            .insert(subject.governance_id);
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
        evaluator_response: Signed<EvaluationResponse>,
    ) -> Result<(), EventError> {
        // Check that the returned hash matches the hash from the pre-evaluation
        let evaluation_request = match self
            .event_pre_evaluations
            .get(&evaluator_response.content.eval_req_hash)
        {
            Some(preevaluation_event) => preevaluation_event,
            None => return Err(EventError::CryptoError(String::from(
                "The hash of the event pre-evaluation does not match any of the pre-evaluations",
            ))),
        };

        let subject_id = match &evaluation_request.event_request.content {
            // The transfer is not evaluated
            EventRequest::Transfer(_) => return Err(EventError::NoEvaluationForTransferEvents),
            EventRequest::EOL(_) => return Err(EventError::NoEvaluationForEOLEvents),
            EventRequest::Create(_) => {
                return Err(EventError::EvaluationOrApprovationInCreationEvent)
            }
            EventRequest::Fact(state_request) => state_request.subject_id.clone(),
        };
        // Look at the status of the event, whether it is under evaluation or not.
        let Some((ValidationStage::Evaluate, signers, quorum_size)) =
            self.subjects_completing_event.get(&subject_id)
        else {
            return Err(EventError::WrongEventPhase);
        };
        let signer = evaluator_response.signature.signer.clone();
        // Comprobar si el evaluador está en la lista de evaluadores
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of evaluators or we already have the signature",
            )));
        }
        // Check that everything is cryptographically correct
        evaluator_response
            .verify()
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
        let evaluation_hash = evaluator_response.content.hash_id()?;
        // Get subject to know if we have it and the subject metadata
        let subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => EventError::SubjectNotFound(subject_id.to_str()),
                _ => EventError::DatabaseError(error.to_string()),
            })?;
        // Check if the governance version matches ours, if not we do not accept it.
        let governance_version = self
            .gov_api
            .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
            .await
            .map_err(EventError::GovernanceError)?;
        // Check that the json patch is valid
        if !hash_match_after_patch(
            &evaluator_response.content,
            evaluator_response.content.patch.clone(),
            subject.properties.clone(),
        )? {
            return Err(EventError::CryptoError(
                "Json patch applied to state hash does not match the new state hash".to_string(),
            ));
        }
        // Save evaluation
        let signatures_set = match self
            .event_evaluations
            .get_mut(&evaluator_response.content.eval_req_hash)
        {
            Some(signatures_set) => {
                insert_or_replace_and_check(
                    signatures_set,
                    (
                        UniqueSignature {
                            signature: evaluator_response.signature.clone(),
                        },
                        evaluator_response.content.eval_success.clone(),
                        evaluation_hash.clone(),
                    ),
                );
                signatures_set
            }
            None => {
                let mut new_signatures_set = HashSet::new();
                new_signatures_set.insert((
                    UniqueSignature {
                        signature: evaluator_response.signature.clone(),
                    },
                    evaluator_response.content.eval_success.clone(),
                    evaluation_hash.clone(),
                ));
                self.event_evaluations.insert(
                    evaluator_response.content.eval_req_hash.clone(),
                    new_signatures_set,
                );
                self.event_evaluations
            .get_mut(&evaluator_response.content.eval_req_hash)
            .expect("Acabamos de insertar el conjunto de firmas, por lo que debe estar presente")
            }
        };
        let (num_signatures_hash_ok, num_signatures_hash_ko) =
            count_signatures_with_event_content_hash(&signatures_set, &evaluation_hash);
        let (quorum_size, negative_quorum_size) = quorum_size.to_owned();
        // Check if we reach Quorum
        let quorum_reached = {
            if num_signatures_hash_ok >= quorum_size {
                Some(true)
            } else if num_signatures_hash_ko >= negative_quorum_size {
                Some(false)
            } else {
                None
            }
        };
        if quorum_reached.is_none() {
            let mut new_signers: HashSet<KeyIdentifier> =
                signers.into_iter().map(|s| s.clone()).collect();
            new_signers.remove(&signer);
            self.ask_signatures(
                &subject_id,
                create_evaluator_request(evaluation_request.clone()),
                new_signers.clone(),
                quorum_size,
            )
            .await?;
            self.subjects_completing_event.insert(
                subject_id,
                (
                    ValidationStage::Evaluate,
                    new_signers,
                    (quorum_size, negative_quorum_size),
                ),
            );
            return Ok(()); // We don't reach quorum, we do nothing
        } else {
            // If so check that json patch applied to the event stop the signature request and start asking for approves with the complete event with the new obtained in this phase if approves are required, otherwise inform validator.
            // Check that when applying Json Patch we reach the final state?
            let evaluator_signatures: HashSet<Signature> = signatures_set
                .iter()
                .filter(|(_, acceptance, hash)| {
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
            let metadata = Metadata {
                namespace: subject.namespace.clone(),
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id.clone(),
                governance_version,
                schema_id: subject.schema_id.clone(),
            };
            if evaluator_response.content.appr_required && !evaluator_response.content.eval_success
            {
                return Err(EventError::ApprovalRequiredWhenEvalFailed);
            }
            // Ask for Approves if necessary, otherwise ask for validations.
            let (stage, event_message) = if evaluator_response.content.appr_required {
                let approval_request = ApprovalRequest {
                    event_request: evaluation_request.event_request.clone(),
                    sn: evaluation_request.sn,
                    gov_version: governance_version,
                    patch: evaluator_response.content.patch,
                    state_hash: evaluator_response.content.state_hash,
                    hash_prev_event,
                    gov_id: subject.governance_id.clone(),
                };
                let approval_request_hash = approval_request.hash_id().map_err(|_| {
                    EventError::CryptoError(String::from(
                        "Error calculating the hash of the proposal",
                    ))
                })?;
                let subject_keys = subject
                    .keys
                    .as_ref()
                    .expect("Llegados a aquí tenemos que ser owner");
                let subject_signature =
                    Signature::new(&approval_request, &subject_keys).map_err(|_| {
                        EventError::CryptoError(String::from("Error signing the Approval Request"))
                    })?;
                let approval_request = Signed::<ApprovalRequest> {
                    content: approval_request,
                    signature: subject_signature,
                };
                // Add to the hashmap to be able to access it when the signatures of the evaluators arrive.
                self.approval_eval_signatures
                    .insert(approval_request_hash.clone(), evaluator_signatures.clone());
                self.approval_requests
                    .insert(approval_request_hash, approval_request.clone());
                let msg = create_approval_request(approval_request);
                // Return TapleMessage directly
                (ValidationStage::Approve, msg)
            } else {
                // No approval required
                let gov_version = self
                    .gov_api
                    .get_governance_version(
                        subject.governance_id.clone(),
                        subject.subject_id.clone(),
                    )
                    .await?;
                let event = Event {
                    subject_id: subject_id.clone(),
                    event_request: evaluation_request.event_request.clone(),
                    sn: evaluation_request.sn,
                    gov_version: governance_version,
                    patch: evaluator_response.content.patch,
                    state_hash: evaluator_response.content.state_hash,
                    eval_success: evaluator_response.content.eval_success,
                    appr_required: evaluator_response.content.appr_required,
                    approved: true,
                    hash_prev_event,
                    evaluators: evaluator_signatures,
                    approvers: HashSet::new(),
                };
                let event_hash = event.hash_id()?;
                let subject_keys = subject
                    .keys
                    .as_ref()
                    .expect("Llegados a aquí tenemos que ser owner");
                let subject_signature = Signature::new(&event, &subject_keys).map_err(|_| {
                    EventError::CryptoError(String::from("Error signing the Event"))
                })?;
                let signed_event = Signed::<Event> {
                    content: event,
                    signature: subject_signature,
                };
                let validation_event =
                    self.create_validation_event(&subject, &signed_event, gov_version)?;
                let event_message = create_validator_request(validation_event.clone());
                self.event_validation_events
                    .insert(event_hash.clone(), validation_event);
                self.events_to_validate
                    .insert(event_hash, signed_event.clone());
                self.database
                    .set_prevalidated_event(&subject.subject_id, signed_event)
                    .map_err(|error| EventError::DatabaseError(error.to_string()))?;
                self.database
                    .del_request(&subject.subject_id)
                    .map_err(|error| EventError::DatabaseError(error.to_string()))?;
                (ValidationStage::Validate, event_message)
            };
            // Clean HashMaps
            self.event_evaluations
                .remove(&evaluator_response.content.eval_req_hash);
            self.event_pre_evaluations
                .remove(&evaluator_response.content.eval_req_hash);
            let (signers, quorum_size) =
                self.get_signers_and_quorum(metadata, stage.clone()).await?;
            self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
                .await?;
            // Make update of the phase the event is going through
            let negative_quorum_size = (signers.len() as u32 - quorum_size) + 1;
            self.subjects_completing_event.insert(
                subject_id.clone(),
                (stage, signers, (quorum_size, negative_quorum_size)),
            );
        }
        Ok(())
    }

    pub async fn approver_signatures(
        &mut self,
        approval: Signed<ApprovalResponse>,
    ) -> Result<(), EventError> {
        // Check the status of the event, if it is in approval or not.
        let approval_request = match self.approval_requests.get(&approval.content.appr_req_hash) {
            Some(event_proposal) => event_proposal,
            None => {
                return Err(EventError::CryptoError(String::from(
                    "The hash of the event proposal does not match any of the proposals",
                )))
            }
        };
        let subject_id = match &approval_request.content.event_request.content {
            // The transfer is not approved
            EventRequest::Transfer(_) => return Err(EventError::NoApprovalForTransferEvents),
            // EOL is not approved
            EventRequest::EOL(_) => return Err(EventError::NoApprovalForEOLEvents),
            EventRequest::Create(_) => {
                return Err(EventError::EvaluationOrApprovationInCreationEvent)
            }
            EventRequest::Fact(state_request) => state_request.subject_id.clone(),
        };
        let Some((ValidationStage::Approve, signers, quorum_size)) =
            self.subjects_completing_event.get(&subject_id)
        else {
            return Err(EventError::WrongEventPhase);
        };
        let signer = approval.signature.signer.clone();
        // Check if approver is in the list of approvers
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of approvers or we already have his approve",
            )));
        }
        // Check that everything is cryptographically correct
        approval
            .verify()
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
        // Get subject to know if we have it and the subject metadata
        let subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => EventError::SubjectNotFound(subject_id.to_str()),
                _ => EventError::DatabaseError(error.to_string()),
            })?;
        // Save approval
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
                self.event_approvations
                    .insert(approval.content.appr_req_hash.clone(), new_approval_set);
                self.event_approvations
            .get_mut(&approval.content.appr_req_hash)
            .expect("Acabamos de insertar el conjunto de approvals, por lo que debe estar presente")
            }
        };
        // Check if we reach positive or negative Quorum
        let num_approvals_with_same_acceptance = approval_set
            .iter()
            .filter(|unique_approval| {
                unique_approval.approval.content.approved == approval.content.approved
            })
            .count() as u32;
        let (quorum_size_now, _) = match approval.content.approved {
            true => (quorum_size.0, true),
            false => (quorum_size.1, false),
        };
        if num_approvals_with_same_acceptance < quorum_size_now {
            // We did not reach quorum for Approval
            let mut new_signers: HashSet<KeyIdentifier> =
                signers.into_iter().map(|s| s.clone()).collect();
            new_signers.remove(&signer);
            self.ask_signatures(
                &subject_id,
                create_approval_request(approval_request.to_owned()),
                signers.clone(),
                quorum_size_now,
            )
            .await?;
            // Make update of the phase the event is going through
            self.subjects_completing_event.insert(
                subject_id,
                (
                    ValidationStage::Approve,
                    new_signers,
                    quorum_size.to_owned(),
                ),
            );
            Ok(()) // We don't reach quorum, we do nothing
        } else {
            let governance_version = self
                .gov_api
                .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
                .await
                .map_err(EventError::GovernanceError)?;
            // If Quorum is reached, we stop asking for approves and start asking for validations with the complete event including the new approves.
            let metadata = Metadata {
                namespace: subject.namespace.clone(),
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id.clone(),
                governance_version,
                schema_id: subject.schema_id.clone(),
            };
            // We create the final event
            let approvals: HashSet<Signature> = approval_set
                .iter()
                .filter(|unique_approval| {
                    unique_approval.approval.content.approved == approval.content.approved
                })
                .map(|approval| approval.approval.signature.clone())
                .collect();
            let event_proposal = self
                .approval_requests
                .get(&approval.content.appr_req_hash)
                .unwrap();
            let gov_version = self
                .gov_api
                .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
                .await?;
            let evaluators = self
                .approval_eval_signatures
                .get(&approval.content.appr_req_hash)
                .unwrap()
                .to_owned();
            let event = Event {
                subject_id: subject_id.clone(),
                event_request: event_proposal.content.event_request.clone(),
                sn: event_proposal.content.sn,
                gov_version,
                patch: event_proposal.content.patch.clone(),
                state_hash: event_proposal.content.state_hash.clone(),
                eval_success: true,
                appr_required: true,
                approved: approval.content.approved,
                hash_prev_event: event_proposal.content.hash_prev_event.clone(),
                evaluators,
                approvers: approvals,
            };
            let event_hash = event.hash_id()?;
            let subject_keys = subject
                .keys
                .as_ref()
                .expect("Llegados a aquí tenemos que ser owner");
            let subject_signature = Signature::new(&event, &subject_keys).map_err(|_| {
                EventError::CryptoError(String::from("Error signing the Event (Approval stage)"))
            })?;
            let signed_event = Signed::<Event> {
                content: event,
                signature: subject_signature,
            };
            let validation_event =
                self.create_validation_event(&subject, &signed_event, gov_version)?;
            let event_message = create_validator_request(validation_event.clone());
            // Clean HashMaps
            self.approval_eval_signatures
                .remove(&approval.content.appr_req_hash);
            self.approval_requests
                .remove(&approval.content.appr_req_hash);
            self.event_approvations
                .remove(&approval.content.appr_req_hash);
            let stage = ValidationStage::Validate;
            let (signers, quorum_size) =
                self.get_signers_and_quorum(metadata, stage.clone()).await?;
            self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
                .await?;
            self.event_validation_events
                .insert(event_hash.clone(), validation_event);
            self.events_to_validate
                .insert(event_hash, signed_event.clone());
            self.database
                .set_prevalidated_event(&subject.subject_id, signed_event)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            self.database
                .del_request(&subject.subject_id)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            // Make update of the phase the event is going through
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
        // Look at the status of the event, whether it is in validation or not.
        let event = match self.events_to_validate.get(&event_hash) {
            Some(event) => event,
            None => {
                return Err(EventError::CryptoError(String::from(
                    "The hash of the event does not match any of the events 1",
                )));
            }
        };
        let validation_event = self
            .event_validation_events
            .get(&event_hash)
            .expect("Should be");
        let subject_id = match &event.content.event_request.content {
            EventRequest::Transfer(transfer_request) => transfer_request.subject_id.clone(),
            EventRequest::EOL(eol_request) => eol_request.subject_id.clone(),
            EventRequest::Create(create_request) => generate_subject_id(
                &create_request.namespace,
                &create_request.schema_id,
                create_request.public_key.to_str(),
                create_request.governance_id.to_str(),
                event.content.gov_version,
            )?,
            EventRequest::Fact(state_request) => state_request.subject_id.clone(),
        };
        // Get subject to know if we have it and the subject metadata
        let subject = match self.database.get_subject(&subject_id) {
            Ok(subject) => Some(subject),
            Err(error) => match error {
                crate::DbError::EntryNotFound => None,
                _ => return Err(EventError::DatabaseError(error.to_string())),
            },
        };
        let (our_governance_version, governance_id) = if event.content.sn == 0 && subject.is_none()
        {
            if let EventRequest::Create(create_request) = &event.content.event_request.content {
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
        } else if subject.is_some() && event.content.sn != 0 {
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
            // We ignore the validation signature because it is not valid, but we ask the validator who sent it to us for governance.
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
            // We ignore the validation signature because it is not valid for us.
            return Ok(());
        }
        // Check phase
        let Some((ValidationStage::Validate, signers, quorum_size)) =
            self.subjects_completing_event.get(&subject_id)
        else {
            return Err(EventError::WrongEventPhase);
        };
        let signer = signature.signer.clone();
        // Check if approver is in the list of approvers
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of validators or we already have the validation",
            )));
        }
        // Check that everything is cryptographically correct
        signature
            .verify(&validation_event.proof)
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
        // Save validation
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
        let quorum_size = quorum_size.to_owned();
        // Check if we reach Quorum and if so stop asking for signatures.
        if (validation_set.len() as u32) < quorum_size.0 {
            let event_message = create_validator_request(validation_event.to_owned());
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
            // Make update of the phase the event is going through
            self.subjects_completing_event.insert(
                subject_id,
                (ValidationStage::Validate, new_signers, quorum_size),
            );
            Ok(())
        } else {
            let validation_signatures: HashSet<Signature> = validation_set
                .iter()
                .map(|unique_signature| unique_signature.signature.clone())
                .collect();
            // If quorum is reached we send it to the ledger.
            if event.content.sn == 0 {
                // TODO: Go from tell to ask to check that it goes well and that there are no weird glitches?
                let response = self
                    .ledger_sender
                    .ask(LedgerCommand::Genesis {
                        event: event.clone(),
                        signatures: validation_signatures,
                        validation_proof: validation_event.proof.clone(),
                    })
                    .await?;
                log::debug!("LEDGER RESPONSE: {:?}", response);
            } else {
                let response = self
                    .ledger_sender
                    .ask(LedgerCommand::OwnEvent {
                        event: event.clone(),
                        signatures: validation_signatures,
                        validation_proof: validation_event.proof.clone(),
                    })
                    .await?;
                log::debug!("LEDGER RESPONSE: {:?}", response);
            }
            self.database
                .del_prevalidated_event(&subject_id)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            self.database
                .del_request(&subject_id)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            self.message_channel
                .tell(MessageTaskCommand::Cancel(String::from(format!(
                    "{}",
                    subject_id.to_str()
                ))))
                .await
                .map_err(EventError::ChannelError)?;
            self.events_to_validate.remove(&event_hash);
            self.event_validation_events.remove(&event_hash);
            self.event_validations.remove(&event_hash);
            self.subjects_completing_event.remove(&subject_id);
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

    fn create_event_prevalidated_no_eval(
        &mut self,
        event_request: Signed<EventRequest>,
        subject: &Subject,
        gov_version: u64,
    ) -> Result<Signed<Event>, EventError> {
        let hash_prev_event = self
            .database
            .get_event(&subject.subject_id, subject.sn)
            .map_err(|e| EventError::DatabaseError(e.to_string()))?
            .content
            .hash_id()?;
        let event = Event {
            subject_id: subject.subject_id.clone(),
            event_request,
            sn: subject.sn + 1,
            gov_version,
            patch: ValueWrapper(
                serde_json::from_str("[]")
                    .map_err(|_| EventError::CryptoError("Error parsing empty json".to_string()))?,
            ),
            state_hash: subject.properties.hash_id()?,
            eval_success: true,
            appr_required: false,
            approved: true,
            hash_prev_event,
            evaluators: HashSet::new(),
            approvers: HashSet::new(),
        };
        let event_content_hash = event.hash_id()?;
        let subject_keys = subject.keys.as_ref().expect("Somos propietario");
        let event_signature = Signature::new(&event, &subject_keys).map_err(|_| {
            EventError::CryptoError(String::from("Error signing the hash of the event content"))
        })?;
        let event = Signed::<Event> {
            content: event,
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
    signatures: &HashSet<(UniqueSignature, bool, DigestIdentifier)>,
    target_event_content_hash: &DigestIdentifier,
) -> (u32, u32) {
    let mut ok: u32 = 0;
    let mut ko: u32 = 0;
    for (_, acceptance, hash) in signatures.iter() {
        if hash == target_event_content_hash {
            match acceptance {
                true => ok += 1,
                false => ko += 1,
            }
        }
    }
    (ok, ko)
}

fn insert_or_replace_and_check<T: PartialEq + Eq + Hash>(
    set: &mut HashSet<T>,
    new_value: T,
) -> bool {
    let replaced = set.remove(&new_value);
    set.insert(new_value);
    replaced
}

fn hash_match_after_patch(
    evaluation: &EvaluationResponse,
    json_patch: ValueWrapper,
    mut prev_properties: ValueWrapper,
) -> Result<bool, EventError> {
    if !evaluation.eval_success {
        let state_hash_calculated = DigestIdentifier::from_serializable_borsh(&prev_properties)
            .map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the state"))
            })?;
        Ok(state_hash_calculated == evaluation.state_hash)
    } else {
        let Ok(patch_json) = serde_json::from_value::<Patch>(json_patch.0) else {
            return Err(EventError::ErrorParsingJsonString(
                "Error Parsing Patch".to_owned(),
            ));
        };
        let Ok(()) = patch(&mut prev_properties.0, &patch_json) else {
            return Err(EventError::ErrorApplyingPatch(
                "Error applying patch".to_owned(),
            ));
        };
        let state_hash_calculated = DigestIdentifier::from_serializable_borsh(&prev_properties)
            .map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the state"))
            })?;
        Ok(state_hash_calculated == evaluation.state_hash)
    }
}
