use std::{
    collections::{HashMap, HashSet},
    ops::Add,
};

use borsh::BorshSerialize;
use json_patch::{patch, Patch};
use serde_json::Value;

use crate::{
    commons::{
        channel::SenderEnd,
        crypto::{KeyPair, Payload, DSA},
        errors::ChannelErrors,
        models::{
            approval::{self, Approval, UniqueApproval},
            event::EventContent,
            event_preevaluation::{Context, EventPreEvaluation},
            event_proposal::{Evaluation, EventProposal, Proposal},
            state::Subject,
            Acceptance,
        },
    },
    event_content::Metadata,
    event_request::EventRequest,
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier, SignatureIdentifier},
    message::{MessageConfig, MessageTaskCommand},
    protocol::command_head_manager::self_signature_manager::SelfSignatureManager,
    signature::{Signature, SignatureContent, UniqueSignature},
    Event, Notification, TimeStamp,
};
use std::hash::Hash;

use super::{errors::EventError, EventCommand, EventMessages, EventResponse};
use crate::database::{DatabaseManager, DB};

const TIMEOUT: u32 = 2000;
const GET_ALL: isize = 200;
const QUORUM_PORCENTAGE_AMPLIFICATION: f64 = 0.2;

pub struct EventCompleter<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    signature_manager: SelfSignatureManager,
    message_channel: SenderEnd<MessageTaskCommand<EventMessages>, ()>,
    notification_sender: tokio::sync::broadcast::Sender<Notification>,
    ledger_sender: SenderEnd<(), ()>,
    own_identifier: KeyIdentifier,
    subjects_by_governance: HashMap<DigestIdentifier, HashSet<DigestIdentifier>>,
    subjects_completing_event:
        HashMap<DigestIdentifier, (ValidationStage, Vec<KeyIdentifier>, u32)>,
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
}

impl<D: DatabaseManager> EventCompleter<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<D>,
        signature_manager: SelfSignatureManager,
        message_channel: SenderEnd<MessageTaskCommand<EventMessages>, ()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        ledger_sender: SenderEnd<(), ()>,
        own_identifier: KeyIdentifier,
    ) -> Self {
        Self {
            gov_api,
            database,
            signature_manager,
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
            own_identifier,
        }
    }

    pub async fn init(&mut self) -> Result<(), EventError> {
        // Fill actual_sn with the last sn of last event created (not necessarily validated) of each subject
        let subjects = self.database.get_all_subjects();
        for subject in subjects.iter() {
            // Comprobar si hay eventos más allá del sn del sujeto que indica que debemos pedir las validaciones porque aún está pendiente de validar
            let last_event = self
                .database
                .get_events_by_range(&subject.subject_id, None, -1)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            // Already know that the vec is contained by 1 element
            let last_event = last_event.pop().unwrap();
            if last_event.content.event_proposal.proposal.sn == subject.sn + 1 {
                let metadata = Metadata {
                    namespace: subject.namespace.clone(),
                    subject_id: subject.subject_id.clone(),
                    governance_id: subject.governance_id.clone(),
                    governance_version: 0, // Not needed
                    schema_id: subject.schema_id.clone(),
                    owner: subject.owner.clone(),
                };
                let stage = ValidationStage::Validate;
                let (signers, quorum_size) =
                    self.get_signers_and_quorum(metadata, stage.clone()).await?;
                let event_message = EventMessages::ValidationRequest(last_event.clone());
                self.ask_signatures(
                    &subject.subject_id,
                    event_message,
                    signers.clone(),
                    quorum_size,
                )
                .await?;
                self.events_to_validate.insert(
                    last_event.signature.content.event_content_hash.clone(),
                    last_event,
                );
                self.subjects_completing_event
                    .insert(subject.subject_id.clone(), (stage, signers, quorum_size));
                continue;
            } else if last_event.content.event_proposal.proposal.sn < subject.sn {
                panic!("Que ha pasado?")
            }
            // Comprobar si hay requests en la base de datos que corresponden con eventos que aun no han llegado a la fase de validación y habría que reiniciar desde pedir evaluaciones
            match self.database.get_request(&subject.subject_id) {
                // TODO: Quitar request_id de este método
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
        version: u64,
    ) -> Result<(), EventError> {
        // Pedir event requests para cada subject_id del set y lanza new_event con ellas
        match self.subjects_by_governance.get(&governance_id) {
            Some(subjects_affected) => {
                for subject_id in subjects_affected.iter() {
                    match self.database.get_request(subject_id) {
                        Ok(event_request) => {
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

    /// Function that is called when a new event request arrives at the system, either invoked by the controller or externally
    pub async fn new_event(
        &mut self,
        event_request: EventRequest,
    ) -> Result<DigestIdentifier, EventError> {
        let subject_id;
        let subject;
        // Check if the content is correct (signature, invoker, etc)
        // Signature check:
        let hash_request = DigestIdentifier::from_serializable_borsh((
            &event_request.request,
            &event_request.timestamp,
        ))
        .map_err(|_| {
            EventError::CryptoError(String::from("Error calculating the hash of the request"))
        })?;
        // Check that the hash is the same
        if hash_request != event_request.signature.content.event_content_hash {
            return Err(EventError::CryptoError(String::from(
                "The request hash does not match the content of the signature",
            )));
        }
        // Check that the signature matches the hash
        match event_request.signature.content.signer.verify(
            &hash_request.derivative(),
            event_request.signature.signature.clone(),
        ) {
            Ok(_) => (),
            Err(_) => {
                return Err(EventError::CryptoError(String::from(
                    "The signature does not validate the request hash",
                )))
            }
        };
        match &event_request.request {
            crate::event_request::EventRequestType::Create(create_request) => {
                // Comprobar si es governance, entonces vale todo, si no comprobar que el invoker soy yo y puedo hacerlo
                if event_request.signature.content.signer != self.own_identifier {
                    return Err(EventError::ExternalGenesisEvent);
                }
                if &create_request.schema_id != "governance" {
                    let creators = self
                        .gov_api
                        .get_signers(
                            Metadata {
                                namespace: create_request.namespace.clone(),
                                subject_id: DigestIdentifier::default(), // Not necessary for this method
                                governance_id: create_request.governance_id.clone(),
                                governance_version: 0, // Not necessary for this method
                                schema_id: create_request.schema_id.clone(),
                                owner: self.own_identifier.clone(),
                            },
                            ValidationStage::Create,
                        )
                        .await
                        .map_err(EventError::GovernanceError)?;
                    if !creators.contains(&self.own_identifier) {
                        return Err(EventError::CreatingPermissionDenied);
                    }
                }
                // Mandar mensaje al ledger de nuevo evento genesis
                // TODO: Enviar al Ledger que hay nuevo evento validado
                // self.ledger_sender
                //     .send(LedgerMessages::Genesis(event_request))
                //     .await
                //     .map_err(EventError::LedgerError)?;
                // TODO: lo dle Ledger, importante
                return Ok(hash_request);
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
            .get_governance_version(subject.governance_id.clone())
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
            },
            ValidationStage::Evaluate,
        );
        let invokator = event_request.signature.content.signer.clone();
        let event_preevaluation = EventPreEvaluation::new(
            event_request,
            Context {
                governance_id: metadata.governance_id.clone(),
                schema_id: metadata.schema_id.clone(),
                invokator,
                creator: subject.creator,
                owner: metadata.owner.clone(),
                actual_state: subject.properties,
                // serde_json::to_string(self.virtual_state.get(&subject_id).unwrap())
                //     .map_err(|_| EventError::ErrorParsingValue)?, // Must be Some, filled in init function
                namespace: metadata.namespace.clone(),
            },
            // self.actual_sn.get(&subject_id).unwrap().to_owned() + 1, // Must be Some, filled in init function
            subject.sn,
        );
        let (signers, quorum_size) = self.get_signers_and_quorum(metadata, stage).await?;
        self.ask_signatures(
            &subject_id,
            EventMessages::EvaluationRequest(event_preevaluation.clone()),
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
        self.event_pre_evaluations
            .insert(event_preevaluation_hash, event_preevaluation);
        // if let Some(sn) = self.actual_sn.get_mut(&subject_id) {
        //     *sn += 1;
        // } else {
        //     unreachable!("Unwraped before")
        // }
        // Add the event to the hashset to not complete two at the same time for the same subject
        self.subjects_completing_event.insert(
            subject_id.clone(),
            (ValidationStage::Evaluate, signers, quorum_size),
        );
        self.subjects_by_governance
            .entry(subject.governance_id)
            .or_insert_with(HashSet::new)
            .insert(subject_id);
        Ok(hash_request)
    }

    pub async fn evaluator_signatures(
        &mut self,
        evaluation: Evaluation,
        json_patch: String,
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
        let subject_id = match &preevaluation_event.event_request.request {
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
        // Check if evaluator is in the list of evaluators
        if !signers.contains(&signature.content.signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of evaluators",
            )));
        }
        // Comprobar que todo es correcto criptográficamente
        let evaluation_hash = check_cryptography(&evaluation, &signature)?;
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
            .get_governance_version(subject.governance_id.clone())
            .await
            .map_err(EventError::GovernanceError)?;
        // Comprobar governance-version que sea la misma que la nuestra
        if governance_version != evaluation.governance_version {
            return Err(EventError::WrongGovernanceVersion);
        }
        // Comprobar que el json patch es válido
        if !hash_match_after_patch(&evaluation.state_hash, &json_patch, &subject.properties)? {
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
        // Comprobar si llegamos a Quorum
        if num_signatures_hash < *quorum_size {
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
            let proposal = Proposal::new(
                preevaluation_event.event_request.clone(),
                preevaluation_event.sn,
                evaluation.clone(),
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
            };
            // Añadir al hashmap para poder acceder a él cuando lleguen las firmas de los evaluadores
            self.event_proposals
                .insert(proposal_hash, event_proposal.clone());
            // Pedir Approves si es necesario, si no pedir validaciones
            let (stage, event_message) = if evaluation.approval_required {
                (
                    ValidationStage::Approve,
                    EventMessages::ApprovalRequest(event_proposal),
                )
            } else {
                let execution = match evaluation.acceptance {
                    crate::commons::models::Acceptance::Ok => true,
                    crate::commons::models::Acceptance::Ko => false,
                    crate::commons::models::Acceptance::Error => false,
                };
                let event_message = self.create_event_prevalidated(
                    event_proposal,
                    HashSet::new(),
                    subject,
                    execution,
                )?;
                self.subjects_by_governance.remove(&subject_id);
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
        // Check if approver is in the list of approvers
        if !signers.contains(&approval.signature.content.signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of approvers",
            )));
        }
        // Comprobar que todo es correcto criptográficamente
        let approval_hash = check_cryptography(&approval.content, &approval.signature)?;
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
                (((signers.len() as u32) - quorum_size) + 1, false)
            }

            crate::commons::models::Acceptance::Error => unreachable!(),
        };
        if num_approvals_with_same_acceptance < quorum_size {
            Ok(()) // No llegamos a quorum, no hacemos nada
        } else {
            // Si se llega a Quorum dejamos de pedir approves y empezamos a pedir notarizaciones con el evento completo incluyendo lo nuevo de las approves
            let metadata = Metadata {
                namespace: subject.namespace.clone(),
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id.clone(),
                governance_version: 0, // Me lo invento porque da igual para estos métodos
                schema_id: subject.schema_id.clone(),
                owner: subject.owner.clone(),
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
            let event_message =
                self.create_event_prevalidated(event_proposal, approvals, subject, execution)?;
            // Limpiar HashMaps
            self.event_approvations
                .remove(&approval.content.event_proposal_hash);
            let stage = ValidationStage::Validate;
            let (signers, quorum_size) =
                self.get_signers_and_quorum(metadata, stage.clone()).await?;
            self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
                .await?;
            self.subjects_by_governance.remove(&subject_id);
            // Hacer update de fase por la que va el evento
            self.subjects_completing_event
                .insert(subject_id, (stage, signers, quorum_size));
            Ok(())
        }
    }

    pub async fn validation_signatures(&mut self, signature: Signature) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en notarización o no
        let event = match self
            .events_to_validate
            .get(&signature.content.event_content_hash)
        {
            Some(event) => event,
            None => {
                return Err(EventError::CryptoError(String::from(
                    "The hash of the event does not match any of the events",
                )))
            }
        };
        let subject_id = match &event.content.event_proposal.proposal.event_request.request {
            crate::event_request::EventRequestType::Create(_) => {
                return Err(EventError::EvaluationOrApprovationInCreationEvent)
            } // Que hago aquí?? devuelvo error?
            crate::event_request::EventRequestType::State(state_request) => {
                state_request.subject_id.clone()
            }
        };
        // CHeck phase
        let Some((ValidationStage::Validate, signers, quorum_size)) = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::WrongEventPhase);
        };
        // Check if approver is in the list of approvers
        if !signers.contains(&signature.content.signer) {
            return Err(EventError::CryptoError(String::from(
                "The signer is not in the list of approvers",
            )));
        }
        // Comprobar que todo es correcto criptográficamente
        let event_hash = check_cryptography(&event.content, &signature)?;
        // Obtener sujeto para saber si lo tenemos y los metadatos del mismo
        let subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => EventError::SubjectNotFound(subject_id.to_str()),
                _ => EventError::DatabaseError(error.to_string()),
            })?;
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
        // Comprobar si llegamos a Quorum y si es así dejar de pedir firmas
        if (validation_set.len() as u32) < *quorum_size {
            Ok(())
        } else {
            let validation_signatures: HashSet<Signature> = validation_set
                .iter()
                .map(|unique_signature| unique_signature.signature.clone())
                .collect();
            // Si se llega a Quorum lo mandamos al ledger
            // TODO: Enviar al Ledger que hay nuevo evento validado
            // self.ledger_sender
            //     .send(LedgerMessages::EventValidated(event, validation_signatures))
            //     .await
            //     .map_err(EventError::LedgerError)?;
            // TODO: lo dle Ledger, importante
            // Limpiar HashMaps
            self.events_to_validate.remove(&event_hash);
            self.event_validations.remove(&event_hash);
            self.subjects_completing_event.remove(&subject_id);
            Ok(())
        }
    }

    async fn get_signers_and_quorum(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<(Vec<KeyIdentifier>, u32), EventError> {
        let signers = self
            .gov_api
            .get_signers(metadata.clone(), stage.clone())
            .await
            .map_err(EventError::GovernanceError)?
            .into_iter()
            .collect();
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
        event_message: EventMessages,
        signers: Vec<KeyIdentifier>,
        quorum_size: u32,
    ) -> Result<(), EventError> {
        let replication_factor = extend_quorum(quorum_size, signers.len());
        self.message_channel
            .tell(MessageTaskCommand::Request(
                Some(String::from(format!("{}", subject_id.to_str()))),
                event_message,
                signers,
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
        subject: Subject,
        execution: bool,
    ) -> Result<EventMessages, EventError> {
        let event_content = EventContent::new(event_proposal, approvals, execution);
        let event_content_hash = DigestIdentifier::from_serializable_borsh(&event_content)
            .map_err(|_| {
                EventError::CryptoError(String::from("Error calculating the hash of the event"))
            })?;
        let subject_keys = subject.keys.expect("Somos propietario");
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
        // TODO: Enviar al Ledger que hay nuevo evento
        // self.ledger_sender
        //     .send(LedgerMessages::Event(event.clone()))
        //     .await
        //     .map_err(EventError::LedgerError)?;
        // TODO: lo dle Ledger, importante
        Ok(EventMessages::ValidationRequest(event))
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
    state_hash: &DigestIdentifier,
    json_patch: &str,
    prev_properties: &str,
) -> Result<bool, EventError> {
    let Ok(patch_json) = serde_json::from_str::<Patch>(json_patch) else {
        return Err(EventError::ErrorParsingJsonString(json_patch.to_owned()));
    };
    let Ok(mut state) = serde_json::from_str::<Value>(prev_properties) else {
        return Err(EventError::ErrorParsingJsonString(prev_properties.to_owned()));
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
    return Ok(state_hash_calculated == *state_hash);
}

fn check_cryptography<T: BorshSerialize>(
    serializable: T,
    signature: &Signature,
) -> Result<DigestIdentifier, EventError> {
    let hash = DigestIdentifier::from_serializable_borsh(&serializable).map_err(|_| {
        EventError::CryptoError(String::from(
            "Error calculating the hash of the serializable",
        ))
    })?;
    if hash != signature.content.event_content_hash {
        return Err(EventError::CryptoError(String::from(
            "The hash does not match the content of the signature",
        )));
    }
    signature
        .content
        .signer
        .verify(&hash.derivative(), signature.signature.clone())
        .map_err(|_| {
            EventError::CryptoError(String::from("The signature does not validate the hash"))
        })?;
    Ok(hash)
}
