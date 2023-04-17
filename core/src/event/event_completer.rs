use std::{
    collections::{HashMap, HashSet},
    ops::Add,
};

use json_patch::{patch, Patch};
use serde_json::Value;

use crate::{
    commons::{
        channel::SenderEnd,
        errors::ChannelErrors,
        models::{
            approval::{self, Approval},
            event_preevaluation::{Context, EventPreEvaluation},
            event_proposal::{Evaluation, EventProposal},
        },
    },
    event_content::Metadata,
    event_request::EventRequest,
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    message::{MessageConfig, MessageTaskCommand},
    protocol::command_head_manager::self_signature_manager::SelfSignatureManager,
    signature::Signature,
    Event, Notification,
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
    subjects_completing_event: HashMap<DigestIdentifier, ValidationStage>,
    // actual_sn: HashMap<DigestIdentifier, u64>,
    event_pre_evaluations: HashMap<DigestIdentifier, EventPreEvaluation>,
    event_evaluations: HashMap<DigestIdentifier, HashSet<Signature>>,
    evaluations_result: HashMap<DigestIdentifier, u32>,
    // virtual_state: HashMap<DigestIdentifier, Value>,
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
            event_pre_evaluations: HashMap::new(),
            event_evaluations: HashMap::new(),
            evaluations_result: HashMap::new(),
            // virtual_state: HashMap::new(),
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
            if last_event.event_proposal.sn == subject.sn + 1 {
            } else if last_event.event_proposal.sn < subject.sn {
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
        self.ask_signatures(
            &subject_id,
            EventMessages::EvaluationRequest(event_preevaluation.clone()),
            &metadata,
            &stage,
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
        self.subjects_completing_event
            .insert(subject_id, ValidationStage::Evaluate);
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
        let Some(&ValidationStage::Evaluate) = self.subjects_completing_event.get(&subject_id) else {
            return Err(EventError::WrongEventPhase);
        };
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
        if !self.event_evaluations
            .entry(evaluation_hash.clone())
            .or_insert(HashSet::new())
            .insert(signature) {
                log::debug!("Evaluation ya estaba presente");
                return Ok(())
            }
        let num_evaluations = self
            .evaluations_result
            .entry(evaluation_hash.clone())
            .and_modify(|counter| *counter += 1)
            .or_insert(1);
        // Comprobar si llegamos a Quorum y si es así comprobar que json patch aplicado al eventoparar la petición de firmas y empezar a pedir las approves con el evento completo con lo nuevo obtenido en esta fase si se requieren approves, si no informar a validator
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

    async fn ask_signatures(
        &self,
        subject_id: &DigestIdentifier,
        event_message: EventMessages,
        metadata: &Metadata,
        stage: &ValidationStage,
    ) -> Result<(), EventError> {
        let signers: Vec<KeyIdentifier> = self
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
