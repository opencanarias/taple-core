use std::{
    collections::{HashMap, HashSet},
    ops::Add,
};

use json_patch::{patch, Patch};
use serde_json::Value;

use crate::{
    commons::{
        channel::SenderEnd,
        crypto::{Payload, DSA},
        errors::ChannelErrors,
        models::{
            approval::{self, Approval},
            event::EventContent,
            event_preevaluation::{Context, EventPreEvaluation},
            event_proposal::{Evaluation, EventProposal, Proposal},
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
    subjects_completing_event:
        HashMap<DigestIdentifier, (ValidationStage, Vec<KeyIdentifier>, u32)>,
    // actual_sn: HashMap<DigestIdentifier, u64>,
    // virtual_state: HashMap<DigestIdentifier, Value>,
    // Evaluation HashMaps
    event_pre_evaluations: HashMap<DigestIdentifier, EventPreEvaluation>,
    event_evaluations: HashMap<DigestIdentifier, HashSet<UniqueSignature>>,
    // Approval HashMaps
    event_proposals: HashMap<DigestIdentifier, EventProposal>,
    // Validation HashMaps
}

impl<D: DatabaseManager> EventCompleter<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<D>,
        signature_manager: SelfSignatureManager,
        message_channel: SenderEnd<MessageTaskCommand<EventMessages>, ()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        ledger_sender: SenderEnd<(), ()>,
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
        }
    }

    pub async fn init(&mut self) -> Result<(), EventError> {
        // Fill actual_sn with the last sn of last event created (not necessarily validated) of each subject
        let subjects = self.database.get_all_subjects();
        for subject in subjects.iter() {
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
            // Comprobar si hay eventes más allá del sn del sujeto que indica que debemos pedir las validaciones porque aún está pendiente de validar
            let last_event = self
                .database
                .get_events_by_range(&subject.subject_id, None, -1)
                .map_err(|error| EventError::DatabaseError(error.to_string()))?;
            // Already know that the vec is contained by 1 element
            let last_event = last_event.pop().unwrap();
            if last_event.content.event_proposal.proposal.sn == subject.sn + 1 {
            } else if last_event.content.event_proposal.proposal.sn < subject.sn {
                panic!("Que ha pasado?")
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

    /// Function that is called when a new event request arrives at the system, either invoked by the controller or externally
    pub async fn new_event(
        &mut self,
        event_request: EventRequest,
    ) -> Result<DigestIdentifier, EventError> {
        let subject_id;
        let subject;
        match &event_request.request {
            crate::event_request::EventRequestType::Create(_) => todo!(),
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
        // Request evaluation signatures, sending request, sn and signature of everything about the subject
        // Get the list of evaluators
        let governance_version = self
            .gov_api
            .get_governance_version(&subject.governance_id)
            .await
            .map_err(EventError::GovernanceError)?;
        let (metadata, stage) = (
            Metadata {
                namespace: subject.namespace,
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id,
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
        let (signers, quorum_size) = self.get_signers_and_quorum(&metadata, &stage).await?;
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
            subject_id,
            (ValidationStage::Evaluate, signers, quorum_size),
        );
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
                return Err(EventError::EvaluationInCreationEvent)
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
        let evaluation_hash =
            DigestIdentifier::from_serializable_borsh(&evaluation).map_err(|_| {
                EventError::CryptoError(String::from(
                    "Error calculating the hash of the evaluation",
                ))
            })?;
        if evaluation_hash != signature.content.event_content_hash {
            return Err(EventError::CryptoError(String::from(
                "The evaluation hash does not match the content of the signature",
            )));
        }
        signature
            .content
            .signer
            .verify(&evaluation_hash.derivative(), signature.signature.clone())
            .map_err(|_| {
                EventError::CryptoError(String::from(
                    "The signature does not validate the evaluation hash",
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
            .get_governance_version(&subject.governance_id)
            .await
            .map_err(EventError::GovernanceError)?;
        // Comprobar governance-version que sea la misma que la nuestra
        if governance_version != evaluation.governance_version {
            return Err(EventError::WrongGovernanceVersion);
        }
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
            let subject_keys = subject.keys.expect("Llegados a aquí tenemos que ser owner");
            let subject_signature = subject_keys
                .sign(Payload::Buffer(proposal_hash.derivative()))
                .map_err(|_| {
                    EventError::CryptoError(String::from("Error signing the hash of the proposal"))
                })?;
            let subject_signature = Signature {
                content: SignatureContent {
                    signer: subject.public_key.clone(),
                    event_content_hash: proposal_hash,
                    timestamp: TimeStamp::now(),
                },
                signature: SignatureIdentifier::new(
                    subject.public_key.to_signature_derivator(),
                    &subject_signature,
                ),
            };
            let event_proposal = EventProposal::new(proposal, subject_signature);
            let metadata = Metadata {
                namespace: subject.namespace,
                subject_id: subject_id.clone(),
                governance_id: subject.governance_id,
                governance_version,
                schema_id: subject.schema_id,
                owner: subject.owner,
            };
            // Limpiar HashMaps
            self.event_evaluations
                .remove(&evaluation.preevaluation_hash);
            self.event_pre_evaluations
                .remove(&evaluation.preevaluation_hash);
            // Pedir Approves si es necesario, si no pedir validaciones
            let (stage, event_message) = if evaluation.approval_required {
                (
                    ValidationStage::Approve,
                    EventMessages::ApprovalRequest(event_proposal),
                )
            } else {
                let event_content = EventContent::new(
                    event_proposal,
                    HashSet::new(),
                    match evaluation.acceptance {
                        crate::commons::models::Acceptance::Ok => true,
                        crate::commons::models::Acceptance::Ko => false,
                        crate::commons::models::Acceptance::Error => false,
                    },
                );
                let event_content_hash = DigestIdentifier::from_serializable_borsh(&event_content)
                    .map_err(|_| {
                        EventError::CryptoError(String::from(
                            "Error calculating the hash of the event",
                        ))
                    })?;
                let event_signature = subject_keys
                    .sign(Payload::Buffer(event_content_hash.derivative()))
                    .map_err(|_| {
                        EventError::CryptoError(String::from(
                            "Error signing the hash of the event content",
                        ))
                    })?;
                let event_signature = Signature {
                    content: SignatureContent {
                        signer: subject.public_key.clone(),
                        event_content_hash: event_content_hash,
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
                (
                    ValidationStage::Validate,
                    EventMessages::ValidationRequest(event),
                )
            };
            let (signers, quorum_size) = self.get_signers_and_quorum(&metadata, &stage).await?;
            self.ask_signatures(&subject_id, event_message, signers.clone(), quorum_size)
                .await?;
        }
        todo!();
    }

    pub fn approver_signatures(&mut self, approval: Approval) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en aprovación o no
        // Comprobar si llegamos a Quorum positivo o negativo
        // Si se llega a Quorum dejamos de pedir approves y empezamos a pedir notarizaciones con el evento completo incluyendo lo nuevo de las approves
        // Actualizar ultimo sn y virtual properties
        todo!();
    }

    pub fn notary_signatures(&mut self, signature: Signature) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en notarización o no
        // Comprobar si llegamos a Quorum y si es así dejar de pedir firmas
        // Si se llega a Quorum creamos el evento final, lo firmamos y lo mandamos al ledger
        // El ledger se encarga de mandarlo a los testigos o somos nosotros?
        todo!();
    }

    async fn get_signers_and_quorum(
        &self,
        metadata: &Metadata,
        stage: &ValidationStage,
    ) -> Result<(Vec<KeyIdentifier>, u32), EventError> {
        let signers = self
            .gov_api
            .get_signers(&metadata, stage)
            .await
            .map_err(EventError::GovernanceError)?
            .into_iter()
            .collect();
        let quorum_size = self
            .gov_api
            .get_quorum(&metadata, stage)
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

fn insert_or_replace_and_check(
    set: &mut HashSet<UniqueSignature>,
    new_value: UniqueSignature,
) -> bool {
    let replaced = set.remove(&new_value); // Si existe un valor igual, lo eliminamos y devolvemos true.
    set.insert(new_value); // Insertamos el nuevo valor.
    replaced // Devolvemos si se ha reemplazado un elemento existente.
}
