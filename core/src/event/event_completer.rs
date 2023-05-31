use std::collections::{HashMap, HashSet};

use json_patch::{patch, Patch};
use serde_json::Value;

use crate::{
    commons::{
        channel::SenderEnd,
        crypto::{check_cryptography, Payload, DSA},
        models::{
            approval::{Approval, UniqueApproval},
            event::{EventContent, ValidationProof},
            event_preevaluation::{Context, EventPreEvaluation},
            event_proposal::{Evaluation, EventProposal, Proposal},
            state::Subject,
            Acceptance,
        },
        self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
    },
    event_content::Metadata,
    event_request::{EventRequest, TransferRequest},
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier, SignatureIdentifier},
    ledger::{LedgerCommand, LedgerResponse},
    message::{MessageConfig, MessageTaskCommand},
    notary::NotaryEvent,
    protocol::protocol_message_manager::TapleMessages,
    signature::{Signature, SignatureContent, UniqueSignature},
    utils::message::{
        approval::create_approval_request, evaluator::create_evaluator_request,
        validation::create_validator_request,
    },
    Event, EventRequestType, Notification, TimeStamp,
};
use std::hash::Hash;

use super::errors::EventError;
use crate::database::{DatabaseManager, DB};

const TIMEOUT: u32 = 2000;
// const GET_ALL: isize = 200;
const QUORUM_PORCENTAGE_AMPLIFICATION: f64 = 0.2;

pub struct EventCompleter<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    notification_sender: tokio::sync::broadcast::Sender<Notification>,
    ledger_sender: SenderEnd<LedgerCommand, LedgerResponse>,
    own_identifier: KeyIdentifier,
    subjects_by_governance: HashMap<DigestIdentifier, HashSet<DigestIdentifier>>,
    subjects_completing_event:
        HashMap<DigestIdentifier, (ValidationStage, HashSet<KeyIdentifier>, u32)>,
    // actual_sn: HashMap<DigestIdentifier, u64>,
    // virtual_state: HashMap<DigestIdentifier, Value>,
    // Evaluation HashMaps
    event_pre_evaluations: HashMap<DigestIdentifier, EventPreEvaluation>,
    event_evaluations: HashMap<DigestIdentifier, HashSet<UniqueSignature>>,
    // Approval HashMaps
    event_proposals: HashMap<DigestIdentifier, EventProposal>,
    event_approvations: HashMap<DigestIdentifier, HashSet<UniqueApproval>>,
    // Validation HashMaps
    events_to_validate: HashMap<DigestIdentifier, Event>,
    event_validations: HashMap<DigestIdentifier, HashSet<UniqueSignature>>,
    event_notary_events: HashMap<DigestIdentifier, NotaryEvent>,
    // SignatureManager
    signature_manager: SelfSignatureManager,
}

impl<D: DatabaseManager> EventCompleter<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<D>,
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

    fn create_notary_event(
        &self,
        subject: &Subject,
        event: &Event,
        gov_version: u64,
    ) -> Result<NotaryEvent, EventError> {
        let state_hash = match &event.content.event_proposal.proposal.event_request.request {
            EventRequestType::Create(_) => {
                unreachable!();
            },
            EventRequestType::State(_) => {
                subject.state_hash_after_apply(&event.content.event_proposal.proposal.json_patch)?
            },
            EventRequestType::Transfer(_) => {
                subject.get_state_hash()?
            }
        };
        let prev_event_hash = if event.content.event_proposal.proposal.sn == 0 {
            DigestIdentifier::default()
        } else {
            self.database
                .get_event(
                    &subject.subject_id,
                    event.content.event_proposal.proposal.sn - 1,
                )
                .map_err(|e| EventError::DatabaseError(e.to_string()))?
                .signature
                .content
                .event_content_hash
        };
        let event_hash = event.signature.content.event_content_hash.clone();
        let proof = match &event.content.event_proposal.proposal.event_request.request {
            EventRequestType::Create(_) | EventRequestType::State(_) => ValidationProof::new(
                subject,
                event.content.event_proposal.proposal.sn,
                prev_event_hash,
                event_hash,
                state_hash,
                gov_version,
            ),
            EventRequestType::Transfer(transfer_request) => {
                ValidationProof::new_from_transfer_event(
                    subject,
                    event.content.event_proposal.proposal.sn,
                    prev_event_hash,
                    event_hash,
                    state_hash,
                    gov_version,
                    event.signature.content.signer.clone(),
                    transfer_request.public_key.clone(),
                )
            }
        };
        match &subject.keys {
            Some(keys) => {
                let proof_hash =
                    DigestIdentifier::from_serializable_borsh(&proof).map_err(|_| {
                        EventError::CryptoError(String::from(
                            "Error calculating the hash of the proof",
                        ))
                    })?;
                let signature = keys
                    .sign(Payload::Buffer(proof_hash.derivative()))
                    .map_err(|_| {
                        EventError::CryptoError(String::from("Error signing the proof"))
                    })?;
                Ok(NotaryEvent {
                    proof,
                    subject_signature: Signature {
                        content: SignatureContent {
                            signer: self.own_identifier.clone(),
                            event_content_hash: proof_hash,
                            timestamp: TimeStamp::now(),
                        },
                        signature: SignatureIdentifier::new(
                            self.own_identifier.to_signature_derivator(),
                            &signature,
                        ),
                    },
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
                        owner: subject.owner.clone(),
                        creator: subject.creator.clone(),
                    };
                    let stage = ValidationStage::Validate;
                    let (signers, quorum_size) =
                        self.get_signers_and_quorum(metadata, stage.clone()).await?;
                    let notary_event =
                        self.create_notary_event(subject, &last_event, gov_version)?;
                    let event_message = create_validator_request(notary_event.clone());
                    log::warn!(
                        "1 PIDIENDO FIRMAS DE VALIDACION PARA: {}",
                        subject.subject_id.to_str()
                    );
                    self.ask_signatures(
                        &subject.subject_id,
                        event_message,
                        signers.clone(),
                        quorum_size,
                    )
                    .await?;
                    self.event_notary_events.insert(
                        last_event.signature.content.event_content_hash.clone(),
                        notary_event,
                    );
                    self.events_to_validate.insert(
                        last_event.signature.content.event_content_hash.clone(),
                        last_event,
                    );
                    self.subjects_completing_event
                        .insert(subject.subject_id.clone(), (stage, signers, quorum_size));
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
        _version: u64,
    ) -> Result<(), EventError> {
        // Pedir event requests para cada subject_id del set y lanza new_event con ellas
        match self.subjects_by_governance.get(&governance_id).cloned() {
            Some(subjects_affected) => {
                for subject_id in subjects_affected.iter() {
                    match self.database.get_request(subject_id) {
                        Ok(event_request) => {
                            let EventRequestType::State(state_request) = &event_request.request else {
                                return Err(EventError::GenesisInGovUpdate)
                            };
                            let subject_id = state_request.subject_id.clone();
                            self.subjects_completing_event.remove(&subject_id);
                            self.new_event(event_request).await?;
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

    async fn process_transfer_event(
        &mut self,
        event_request: EventRequest,
    ) -> Result<(), EventError> {
        let EventRequestType::Transfer(transfer_request) = &event_request.request else {
            unreachable!();
        };
        // Comprobamos si tenemos el sujeto
        // Esta parte se puede refactorizar
        let subject_id = &transfer_request.subject_id;
        let subject = match self.database.get_subject(subject_id) {
            Ok(subject) => subject,
            Err(error) => match error {
                crate::DbError::EntryNotFound => {
                    return Err(EventError::SubjectNotFound(subject_id.to_str()))
                }
                _ => return Err(EventError::DatabaseError(error.to_string())),
            },
        };
        // Check if we already have an event for that subject
        let None = self.subjects_completing_event.get(subject_id) else {
            return Err(EventError::EventAlreadyInProgress);
        };
        // Obtenemos versión actual de la gobernanza
        let gov_version = self
            .gov_api
            .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
            .await
            .map_err(EventError::GovernanceError)?;
        let (event_proposal, proposal_hash) = self
            .generate_event_proposal(&event_request, &subject, gov_version)
            .await?;
        let metadata = Metadata {
            namespace: subject.namespace.clone(),
            subject_id: subject_id.clone(),
            governance_id: subject.governance_id.clone(),
            governance_version: gov_version,
            schema_id: subject.schema_id.clone(),
            owner: event_request.signature.content.signer.clone(),
            creator: subject.creator.clone(),
        };
        // Añadir al hashmap para poder acceder a él cuando lleguen las firmas de los validadores
        self.event_proposals
            .insert(proposal_hash, event_proposal.clone());
        let event = &self.create_event_prevalidated(
            event_proposal,
            HashSet::new(),
            &subject,
            true, // TODO: Consultar
        )?;
        log::error!("PRE NOTARY");
        let notary_event = self.create_notary_event(&subject, &event, gov_version)?;
        log::error!("POST NOTARY");
        let event_message = create_validator_request(notary_event.clone());
        self.event_notary_events.insert(
            event.signature.content.event_content_hash.clone(),
            notary_event,
        );
        //(ValidationStage::Validate, event_message)
        let stage = ValidationStage::Validate;
        let (signers, quorum_size) = self.get_signers_and_quorum(metadata, stage.clone()).await?;
        log::warn!(
            "2 PIDIENDO FIRMAS DE VALIDACION PARA: {}",
            subject.subject_id.to_str()
        );
        self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
            .await?;
        // Hacer update de fase por la que va el evento
        self.subjects_completing_event
            .insert(subject_id.clone(), (stage, signers, quorum_size));
        Ok(())
    }

    async fn generate_event_proposal(
        &self,
        event_request: &EventRequest,
        subject: &Subject,
        gov_version: u64,
    ) -> Result<(EventProposal, DigestIdentifier), EventError> {
        let hash_prev_event = self
            .database
            .get_event(&subject.subject_id, subject.sn)
            .map_err(|e| EventError::DatabaseError(e.to_string()))?
            .signature
            .content
            .event_content_hash;
        let proposal = Proposal::new(
            event_request.clone(),
            subject.sn + 1,
            hash_prev_event,
            gov_version,
            None,
            String::from(""),
            HashSet::new(),
        );
        let proposal_hash = DigestIdentifier::from_serializable_borsh(&proposal).map_err(|_| {
            EventError::CryptoError(String::from("Error calculating the hash of the proposal"))
        })?;
        let subject_keys = subject
            .keys
            .clone()
            .expect("Llegados a aquí tenemos que ser owner");
        let subject_signature = subject_keys
            .sign(Payload::Buffer(proposal_hash.derivative()))
            .map_err(|_| {
                EventError::CryptoError(String::from("Error signing the hash of the proposal"))
            })?;
        let subject_signature = Signature {
            content: SignatureContent {
                signer: subject.public_key.clone(),
                event_content_hash: proposal_hash.clone(),
                timestamp: TimeStamp::now(),
            },
            signature: SignatureIdentifier::new(
                subject.public_key.to_signature_derivator(),
                &subject_signature,
            ),
        };
        Ok((
            EventProposal::new(proposal, subject_signature),
            proposal_hash,
        ))
    }

    /// Function that is called when a new event request arrives at the system, either invoked by the controller or externally
    pub async fn new_event(
        &mut self,
        event_request: EventRequest,
    ) -> Result<DigestIdentifier, EventError> {
        let subject_id;
        let subject;
        // Check if the content is correct (signature, invoker, etc)
        // Signature check:
        event_request
            .check_signatures()
            .map_err(EventError::SubjectError)?;
        match &event_request.request {
            crate::event_request::EventRequestType::Transfer(_) => {
                log::error!("ENTRA EN TRANSFER");
                // Debemos eliminar el material criptográfico del sujeto actual y cambiar su clave pública
                // No obstante, el evento debe firmarse con la clave actual, por lo que no se puede eliminar
                // de inmediato. Debe ser eliminado pues, después de la validación.
                // Estos eventos ni se evaluan, ni se aprueban.
                // No es necesario comprobar la governance, pues no se requieren permisos para la transferencia
                let er_hash = event_request.signature.content.event_content_hash.clone();
                self.process_transfer_event(event_request).await?;
                return Ok(er_hash);
            }
            crate::event_request::EventRequestType::Create(create_request) => {
                // Comprobar si es governance, entonces vale todo, si no comprobar que el invoker soy yo y puedo hacerlo
                if event_request.signature.content.signer != self.own_identifier {
                    return Err(EventError::ExternalGenesisEvent);
                }
                if &create_request.schema_id != "governance" {
                    let governance_version = self
                        .gov_api
                        .get_governance_version(
                            create_request.governance_id.clone(),
                            DigestIdentifier::default(),
                        )
                        .await
                        .map_err(EventError::GovernanceError)?;
                    let creators = self
                        .gov_api
                        .get_signers(
                            Metadata {
                                namespace: create_request.namespace.clone(),
                                subject_id: DigestIdentifier::default(), // Not necessary for this method
                                governance_id: create_request.governance_id.clone(),
                                governance_version,
                                schema_id: create_request.schema_id.clone(),
                                owner: self.own_identifier.clone(),
                                creator: self.own_identifier.clone(),
                            },
                            ValidationStage::Create,
                        )
                        .await
                        .map_err(EventError::GovernanceError)?;
                    if !creators.contains(&self.own_identifier) {
                        return Err(EventError::CreatingPermissionDenied);
                    }
                }
                let request_hash = event_request.signature.content.event_content_hash.clone();
                self.ledger_sender
                    .tell(LedgerCommand::Genesis { event_request })
                    .await?;
                return Ok(request_hash);
            }
            crate::event_request::EventRequestType::State(state_request) => {
                // Check if we have the subject in the database
                subject_id = state_request.subject_id.to_owned();
                subject = match self.database.get_subject(&subject_id) {
                    Ok(subject) => subject,
                    Err(error) => match error {
                        crate::DbError::EntryNotFound => {
                            return Err(EventError::SubjectNotFound(subject_id.to_str()))
                        }
                        _ => return Err(EventError::DatabaseError(error.to_string())),
                    },
                };
                // Check if we already have an event for that subject
                let None = self.subjects_completing_event.get(&subject_id) else {
                    return Err(EventError::EventAlreadyInProgress);
                };
            }
        };
        // Chek if we are owner of Subject
        if subject.keys.is_none() {
            return Err(EventError::SubjectNotOwned(subject_id.to_str()));
        }
        // Request evaluation signatures, sending request, sn and signature of everything about the subject
        // Get the list of evaluators
        let governance_version = self
            .gov_api
            .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
            .await
            .map_err(EventError::GovernanceError)?;
        let (metadata, stage) = (
            Metadata {
                namespace: subject.namespace,
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id.clone(),
                governance_version,
                schema_id: subject.schema_id,
                owner: subject.owner,
                creator: subject.creator.clone(),
            },
            ValidationStage::Evaluate,
        );
        let event_preevaluation = EventPreEvaluation {
            event_request: event_request.clone(),
            context: Context {
                governance_id: metadata.governance_id.clone(),
                schema_id: metadata.schema_id.clone(),
                creator: subject.creator,
                owner: metadata.owner.clone(),
                actual_state: subject.properties,
                // serde_json::to_string(self.virtual_state.get(&subject_id).unwrap())
                //     .map_err(|_| EventError::ErrorParsingValue)?, // Must be Some, filled in init function
                namespace: metadata.namespace.clone(),
                governance_version,
            },
            sn: subject.sn + 1,
            // self.actual_sn.get(&subject_id).unwrap().to_owned() + 1, // Must be Some, filled in init function
        };
        let (signers, quorum_size) = self.get_signers_and_quorum(metadata, stage.clone()).await?;
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
        self.ask_signatures(
            &subject_id,
            create_evaluator_request(event_preevaluation.clone()),
            signers.clone(),
            quorum_size,
        )
        .await?;
        let event_preevaluation_hash =
            DigestIdentifier::from_serializable_borsh(&event_preevaluation).map_err(|_| {
                EventError::CryptoError(String::from(
                    "Error calculating the hash of the event pre-evaluation",
                ))
            })?;
        let er_hash = event_request.signature.content.event_content_hash.clone();
        self.database
            .set_request(&subject.subject_id, event_request)
            .map_err(|error| EventError::DatabaseError(error.to_string()))?;
        self.event_pre_evaluations
            .insert(event_preevaluation_hash, event_preevaluation);
        // if let Some(sn) = self.actual_sn.get_mut(&subject_id) {
        //     *sn += 1;
        // } else {
        //     unreachable!("Unwraped before")
        // }
        // Add the event to the hashset to not complete two at the same time for the same subject
        self.subjects_completing_event
            .insert(subject_id.clone(), (stage, signers, quorum_size));
        self.subjects_by_governance
            .entry(subject.governance_id)
            .or_insert_with(HashSet::new)
            .insert(subject_id);
        Ok(er_hash)
    }

    pub async fn evaluator_signatures(
        &mut self,
        evaluation: Evaluation,
        json_patch: String,
        signature: Signature,
    ) -> Result<(), EventError> {
        log::warn!("LLEGAN FIRMAS EVALUACION");
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
        let subject_id = match &preevaluation_event.event_request.request {
            // La transferencia no se evalua
            crate::event_request::EventRequestType::Transfer(_) => {
                return Err(EventError::NoEvaluationForTransferEvents)
            }
            crate::event_request::EventRequestType::Create(_) => {
                return Err(EventError::EvaluationOrApprovationInCreationEvent)
            } // Que hago aquí?? devuelvo error?
            crate::event_request::EventRequestType::State(state_request) => {
                state_request.subject_id.clone()
            }
        };
        // Mirar en que estado está el evento, si está en evaluación o no
        let Some((ValidationStage::Evaluate, signers, quorum_size)) = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::WrongEventPhase);
        };
        let signer = signature.content.signer.clone();
        // Check if evaluator is in the list of evaluators
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of evaluators or we already have the signature",
            )));
        }
        // Comprobar que todo es correcto criptográficamente
        let evaluation_hash = check_cryptography(&evaluation, &signature)
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
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
        if !hash_match_after_patch(&evaluation, &json_patch, &subject.properties)? {
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
                insert_or_replace_and_check(signatures_set, UniqueSignature { signature });
                signatures_set
            }
            None => {
                let mut new_signatures_set = HashSet::new();
                new_signatures_set.insert(UniqueSignature { signature });
                self.event_evaluations
                    .insert(evaluation.preevaluation_hash.clone(), new_signatures_set);
                self.event_evaluations
            .get_mut(&evaluation.preevaluation_hash)
            .expect("Acabamos de insertar el conjunto de firmas, por lo que debe estar presente")
            }
        };
        let num_signatures_hash =
            count_signatures_with_event_content_hash(&signatures_set, &evaluation_hash) as u32;
        let quorum_size = quorum_size.to_owned();
        // Comprobar si llegamos a Quorum
        if num_signatures_hash < quorum_size {
            log::warn!("NO QUORUM EN EVALUACIÓN");
            log::warn!("QUORUM EVAL: {}", quorum_size);
            let mut new_signers: HashSet<KeyIdentifier> =
                signers.into_iter().map(|s| s.clone()).collect();
            new_signers.remove(&signer);
            log::warn!(
                "PIDIENDO FIRMAS DE EVALUACIÓN PARA: {}",
                subject.subject_id.to_str()
            );
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
                (ValidationStage::Evaluate, new_signers, quorum_size),
            );
            return Ok(()); // No llegamos a quorum, no hacemos nada
        } else {
            // Si es así comprobar que json patch aplicado al evento parar la petición de firmas y empezar a pedir las approves con el evento completo con lo nuevo obtenido en esta fase si se requieren approves, si no informar a validator
            // Comprobar que al aplicar Json Patch llegamos al estado final?
            // Crear Event Proposal
            let evaluator_signatures = signatures_set
                .iter()
                .filter(|signature| {
                    signature.signature.content.event_content_hash == evaluation_hash
                })
                .map(|signature| signature.signature.clone())
                .collect();
            let hash_prev_event = self
                .database
                .get_event(&subject.subject_id, subject.sn)
                .map_err(|e| EventError::DatabaseError(e.to_string()))?
                .signature
                .content
                .event_content_hash;
            let proposal = Proposal::new(
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
            let subject_signature = subject_keys
                .sign(Payload::Buffer(proposal_hash.derivative()))
                .map_err(|_| {
                    EventError::CryptoError(String::from("Error signing the hash of the proposal"))
                })?;
            let subject_signature = Signature {
                content: SignatureContent {
                    signer: subject.public_key.clone(),
                    event_content_hash: proposal_hash.clone(),
                    timestamp: TimeStamp::now(),
                },
                signature: SignatureIdentifier::new(
                    subject.public_key.to_signature_derivator(),
                    &subject_signature,
                ),
            };
            let event_proposal = EventProposal::new(proposal, subject_signature);
            let metadata = Metadata {
                namespace: subject.namespace.clone(),
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id.clone(),
                governance_version,
                schema_id: subject.schema_id.clone(),
                owner: subject.owner.clone(),
                creator: subject.creator.clone(),
            };
            // Añadir al hashmap para poder acceder a él cuando lleguen las firmas de los evaluadores
            self.event_proposals
                .insert(proposal_hash, event_proposal.clone());
            // Pedir Approves si es necesario, si no pedir validaciones
            let (stage, event_message) = if evaluation.approval_required {
                log::warn!(
                    "PIDIENDO FIRMAS DE APROBACION PARA: {}",
                    subject.subject_id.to_str()
                );
                let msg = create_approval_request(event_proposal);
                // Retornar TapleMessage directamente
                (ValidationStage::Approve, msg)
            } else {
                // No se necesita aprobación
                let execution = match evaluation.acceptance {
                    crate::commons::models::Acceptance::Ok => true,
                    crate::commons::models::Acceptance::Ko => false,
                    crate::commons::models::Acceptance::Error => false,
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
                let notary_event = self.create_notary_event(&subject, &event, gov_version)?;
                let event_message = create_validator_request(notary_event.clone());
                self.event_notary_events.insert(
                    event.signature.content.event_content_hash.clone(),
                    notary_event,
                );
                log::warn!(
                    "POST EVAL: PIDIENDO FIRMAS DE VALIDACION PARA: {}",
                    subject.subject_id.to_str()
                );
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
            self.subjects_completing_event
                .insert(subject_id, (stage, signers, quorum_size));
        }
        Ok(())
    }

    pub async fn approver_signatures(&mut self, approval: Approval) -> Result<(), EventError> {
        if let Acceptance::Error = approval.content.acceptance {
            return Ok(()); // Ignoramos respuestas de Error
        }
        // Mirar en que estado está el evento, si está en aprovación o no
        let event_proposal = match self
            .event_proposals
            .get(&approval.content.event_proposal_hash)
        {
            Some(event_proposal) => event_proposal,
            None => {
                return Err(EventError::CryptoError(String::from(
                    "The hash of the event proposal does not match any of the proposals",
                )))
            }
        };
        let subject_id = match &event_proposal.proposal.event_request.request {
            // La transferencia no se aprueba
            crate::event_request::EventRequestType::Transfer(_) => {
                return Err(EventError::NoAprovalForTransferEvents)
            }
            crate::event_request::EventRequestType::Create(_) => {
                return Err(EventError::EvaluationOrApprovationInCreationEvent)
            } // Que hago aquí?? devuelvo error?
            crate::event_request::EventRequestType::State(state_request) => {
                state_request.subject_id.clone()
            }
        };
        let Some((ValidationStage::Approve, signers, quorum_size)) = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::WrongEventPhase);
        };
        let signer = approval.signature.content.signer.clone();
        // Check if approver is in the list of approvers
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of approvers or we already have his approve",
            )));
        }
        // Comprobar que todo es correcto criptográficamente
        let _approval_hash = check_cryptography(&approval.content, &approval.signature)
            .map_err(|error| EventError::CryptoError(error.to_string()))?;
        // Obtener sujeto para saber si lo tenemos y los metadatos del mismo
        let subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => EventError::SubjectNotFound(subject_id.to_str()),
                _ => EventError::DatabaseError(error.to_string()),
            })?;
        // Guardar aprobación
        let approval_set = match self
            .event_approvations
            .get_mut(&approval.content.event_proposal_hash)
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
                    approval.content.event_proposal_hash.clone(),
                    new_approval_set,
                );
                self.event_approvations
            .get_mut(&approval.content.event_proposal_hash)
            .expect("Acabamos de insertar el conjunto de approvals, por lo que debe estar presente")
            }
        };
        // Comprobar si llegamos a Quorum positivo o negativo
        let num_approvals_with_same_acceptance = approval_set
            .iter()
            .filter(|unique_approval| {
                unique_approval.approval.content.acceptance == approval.content.acceptance
            })
            .count() as u32;
        let (quorum_size, execution) = match approval.content.acceptance {
            crate::commons::models::Acceptance::Ok => (quorum_size.to_owned(), true),
            crate::commons::models::Acceptance::Ko => {
                (((signers.len() as u32) - *quorum_size) + 1, false)
            }
            crate::commons::models::Acceptance::Error => unreachable!(),
        };
        if num_approvals_with_same_acceptance < quorum_size {
            let mut new_signers: HashSet<KeyIdentifier> =
                signers.into_iter().map(|s| s.clone()).collect();
            new_signers.remove(&signer);
            log::warn!(
                "faltan PIDIENDO FIRMAS DE Aprobacion PARA: {}",
                subject.subject_id.to_str()
            );
            self.ask_signatures(
                &subject_id,
                create_approval_request(event_proposal.to_owned()),
                signers.clone(),
                quorum_size,
            )
            .await?;
            // Hacer update de fase por la que va el evento
            self.subjects_completing_event.insert(
                subject_id,
                (ValidationStage::Evaluate, new_signers, quorum_size),
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
                owner: subject.owner.clone(),
                creator: subject.creator.clone(),
            };
            // Creamos el evento final
            let approvals = approval_set
                .iter()
                .filter(|unique_approval| {
                    unique_approval.approval.content.acceptance == approval.content.acceptance
                })
                .map(|approval| approval.approval.clone())
                .collect();
            let event_proposal = self
                .event_proposals
                .remove(&approval.content.event_proposal_hash)
                .unwrap();

            let gov_version = self
                .gov_api
                .get_governance_version(subject.governance_id.clone(), subject.subject_id.clone())
                .await?;
            let event =
                &self.create_event_prevalidated(event_proposal, approvals, &subject, execution)?;
            let notary_event = self.create_notary_event(&subject, &event, gov_version)?;
            let event_message = create_validator_request(notary_event.clone());

            // Limpiar HashMaps
            self.event_approvations
                .remove(&approval.content.event_proposal_hash);
            let stage = ValidationStage::Validate;
            let (signers, quorum_size) =
                self.get_signers_and_quorum(metadata, stage.clone()).await?;
            log::warn!(
                "START PIDIENDO FIRMAS DE VALIDACION PARA: {}",
                subject.subject_id.to_str()
            );
            self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
                .await?;
            self.event_notary_events.insert(
                event.signature.content.event_content_hash.clone(),
                notary_event,
            );
            // Hacer update de fase por la que va el evento
            self.subjects_completing_event
                .insert(subject_id, (stage, signers, quorum_size));
            Ok(())
        }
    }

    pub async fn validation_signatures(
        &mut self,
        event_hash: DigestIdentifier,
        signature: Signature,
    ) -> Result<(), EventError> {
        log::warn!("VALIDATION SIGNATURES");
        // Mirar en que estado está el evento, si está en notarización o no
        let event = match self.events_to_validate.get(&event_hash) {
            Some(event) => event,
            None => {
                return Err(EventError::CryptoError(String::from(
                    "The hash of the event does not match any of the events",
                )))
            }
        };
        let notary_event = self
            .event_notary_events
            .get(&event_hash)
            .expect("Should be");
        let subject_id = match &event.content.event_proposal.proposal.event_request.request {
            crate::event_request::EventRequestType::Transfer(transfer_request) => {
                transfer_request.subject_id.clone()
            }
            crate::event_request::EventRequestType::Create(_) => {
                return Err(EventError::EvaluationOrApprovationInCreationEvent)
            } // Que hago aquí?? devuelvo error?
            crate::event_request::EventRequestType::State(state_request) => {
                state_request.subject_id.clone()
            }
        };
        log::warn!("TDO BIEN");
        let a = self.subjects_completing_event.get(&subject_id);
        log::warn!("STAGE: {:?}", a);
        // CHeck phase
        let Some((ValidationStage::Validate, signers, quorum_size)) = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::WrongEventPhase);
        };
        log::warn!("PASO 1");
        let signer = signature.content.signer.clone();
        // Check if approver is in the list of approvers
        if !signers.contains(&signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of validators or we already have the validation",
            )));
        }
        log::warn!("PASO 2");
        // Obtener sujeto para saber si lo tenemos y los metadatos del mismo
        let _subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => EventError::SubjectNotFound(subject_id.to_str()),
                _ => EventError::DatabaseError(error.to_string()),
            })?;
        log::warn!("PASO 3");
        // Comprobar que todo es correcto criptográficamente
        let event_hash = check_cryptography(&notary_event.proof, &signature)
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
        if (validation_set.len() as u32) < quorum_size {
            log::warn!("PASO 6 IF");
            let notary_event = match self
                .event_notary_events
                .get(&event.signature.content.event_content_hash)
            {
                Some(notary_event) => notary_event.to_owned(),
                None => {
                    return Err(EventError::CryptoError(String::from(
                        "The hash of the event does not match any of the events",
                    )))
                }
            };
            log::warn!(
                "NO COMPLETAS PIDIENDO FIRMAS DE VALIDACION PARA: {}",
                notary_event.proof.subject_id.to_str()
            );
            let event_message = create_validator_request(notary_event);
            let mut new_signers: HashSet<KeyIdentifier> =
                signers.into_iter().map(|s| s.clone()).collect();
            new_signers.remove(&signer);
            self.ask_signatures(&subject_id, event_message, new_signers.clone(), quorum_size)
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
            self.ledger_sender
                .tell(LedgerCommand::OwnEvent {
                    event: event.clone(),
                    signatures: validation_signatures,
                })
                .await?;
            self.database
                .del_prevalidated_event(&subject_id)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            // Cancelar pedir firmas
            self.message_channel
                .tell(MessageTaskCommand::Cancel(
                    String::from(format!("{}", subject_id.to_str())
            ))).await.map_err(EventError::ChannelError)?;
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
        event_proposal: EventProposal,
        approvals: HashSet<Approval>,
        subject: &Subject,
        execution: bool,
    ) -> Result<Event, EventError> {
        let event_content = EventContent::new(event_proposal, approvals, execution);
        let event_content_hash = DigestIdentifier::from_serializable_borsh(&event_content)
            .map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the event"))
            })?;
        let subject_keys = subject.keys.as_ref().expect("Somos propietario");
        let event_signature = subject_keys
            .sign(Payload::Buffer(event_content_hash.derivative()))
            .map_err(|_| {
                EventError::CryptoError(String::from("Error signing the hash of the event content"))
            })?;
        let event_signature = Signature {
            content: SignatureContent {
                signer: subject.public_key.clone(),
                event_content_hash: event_content_hash.clone(),
                timestamp: TimeStamp::now(),
            },
            signature: SignatureIdentifier::new(
                subject.public_key.to_signature_derivator(),
                &event_signature,
            ),
        };
        let event = Event {
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
    signatures: &HashSet<UniqueSignature>,
    target_event_content_hash: &DigestIdentifier,
) -> usize {
    signatures
        .iter()
        .filter(|signature| {
            signature.signature.content.event_content_hash == *target_event_content_hash
        })
        .count()
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
    json_patch: &str,
    prev_properties: &str,
) -> Result<bool, EventError> {
    let Ok(mut state) = serde_json::from_str::<Value>(prev_properties) else {
    return Err(EventError::ErrorParsingJsonString(prev_properties.to_owned()));
};
    if evaluation.acceptance != Acceptance::Ok {
        let state = serde_json::to_string(&state)
            .map_err(|_| EventError::ErrorParsingJsonString("New State after patch".to_owned()))?;
        let state_hash_calculated =
            DigestIdentifier::from_serializable_borsh(&state).map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the state"))
            })?;
        Ok(state_hash_calculated == evaluation.state_hash)
    } else {
        let Ok(patch_json) = serde_json::from_str::<Patch>(json_patch) else {
            return Err(EventError::ErrorParsingJsonString(json_patch.to_owned()));
    };
        let Ok(()) = patch(&mut state, &patch_json) else {
        return Err(EventError::ErrorApplyingPatch(json_patch.to_owned()));
    };
        let state = serde_json::to_string(&state)
            .map_err(|_| EventError::ErrorParsingJsonString("New State after patch".to_owned()))?;
        let state_hash_calculated =
            DigestIdentifier::from_serializable_borsh(&state).map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the state"))
            })?;
        Ok(state_hash_calculated == evaluation.state_hash)
    }
}
