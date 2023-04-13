use std::collections::{HashMap, HashSet};

use crate::{
    commons::{
        channel::SenderEnd,
        errors::ChannelErrors,
        models::{
            approval::{self, Approval},
            event_preevaluation::{Context, EventPreEvaluation},
            event_proposal::Evaluation,
        },
    },
    event_content::Metadata,
    event_request::EventRequest,
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    message::{MessageConfig, MessageTaskCommand},
    protocol::{
        command_head_manager::self_signature_manager::{
            SelfSignatureInterface, SelfSignatureManager,
        },
        protocol_message_manager::ProtocolManagerMessages,
    },
    signature::Signature,
    Notification,
};

use super::{errors::EventError, EventCommand, EventMessages, EventResponse};
use crate::database::{DatabaseManager, DB};

const TIMEOUT: u32 = 2000;
const QUORUM_PORCENTAGE_AMPLIFICATION: f64 = 0.2;

pub struct EventCompleter<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    signature_manager: SelfSignatureManager,
    message_channel: SenderEnd<MessageTaskCommand<EventMessages>, ()>,
    notification_sender: tokio::sync::broadcast::Sender<Notification>,
    ledger_sender: SenderEnd<(), ()>,
    subjects_completing_event: HashSet<DigestIdentifier>,
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
            subjects_completing_event: HashSet::new(),
        }
    }

    /// Función que se llama cuando llega una nueva event request al sistema, ya sea invocada por el controlador o externamente
    pub async fn new_event(
        &mut self,
        event_request: EventRequest,
    ) -> Result<DigestIdentifier, EventError> {
        let subject_id;
        let subject;
        match &event_request.request {
            crate::event_request::EventRequestType::Create(_) => todo!(),
            crate::event_request::EventRequestType::State(state_request) => {
                // Comprobar si tenemos el sujeto en la base de datos
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
                // Comprobar si ya tenemos un evento para ese sujeto
                let None = self.subjects_completing_event.get(&subject_id) else {
                    return Err(EventError::EventAlreadyInProgress);
                };
            }
        };
        // Comprobar si el contenido es correcto (firma, invokator, etc)
        // Comprobación de firma:
        let hash_request = DigestIdentifier::from_serializable_borsh((
            &event_request.request,
            &event_request.timestamp,
        ))
        .map_err(|_| {
            EventError::CryptoError(String::from("Error al calcular el hash del request"))
        })?;
        // Comprobación de que el hash es el mismo
        if hash_request != event_request.signature.content.event_content_hash {
            return Err(EventError::CryptoError(String::from(
                "El hash del request no coincide con el contenido de la firma",
            )));
        }
        // Comprobación de que la firma coincide con el hash
        match event_request.signature.content.signer.verify(
            &hash_request.derivative(),
            event_request.signature.signature.clone(),
        ) {
            Ok(_) => (),
            Err(_) => {
                return Err(EventError::CryptoError(String::from(
                    "La firma no valida el hash de la request",
                )))
            }
        };
        // Añado el evento al hashset para no completar dos a la vez del mismo sujeto
        self.subjects_completing_event.insert(subject_id.clone());
        // Pedir firmas de evaluación, mandando request, sn y firma del sujeto de todo
        // Obtengo la lista de evaluadores
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
        let evaluators: Vec<KeyIdentifier> = self
            .gov_api
            .get_signers(&metadata, &stage)
            .await
            .map_err(EventError::GovernanceError)?
            .into_iter()
            .collect();
        let quorum_size = self
            .gov_api
            .get_quorum(&metadata, &stage)
            .await
            .map_err(EventError::GovernanceError)?;
        let invokator = event_request.signature.content.signer.clone();
        let event_preevaluation = EventPreEvaluation::new(
            event_request,
            Context {
                governance_id: metadata.governance_id,
                schema_id: metadata.schema_id,
                invokator,
                creator: subject.creator,
                owner: metadata.owner,
                actual_state: subject.properties,
                namespace: metadata.namespace,
            },
            subject.sn + 1,
        );
        let evaluators_len = evaluators.len() as f64;
        let quorum_extended =
            quorum_size + (evaluators_len * QUORUM_PORCENTAGE_AMPLIFICATION).ceil() as u32;
        self.message_channel
            .tell(MessageTaskCommand::Request(
                Some(String::from(format!("E-{}", subject_id.to_str()))),
                EventMessages::EvaluationRequest(event_preevaluation),
                evaluators,
                MessageConfig {
                    timeout: TIMEOUT,
                    replication_factor: quorum_extended as f64 / evaluators_len,
                },
            ))
            .await;
        Ok(hash_request)
    }

    pub fn evaluator_signatures(
        &mut self,
        evaluation: Evaluation,
        signature: Signature,
    ) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en evaluación o no
        // Comprobar si la versión de la governanza coincide con la nuestra, si no no lo aceptamos
        // Comprobar que todo es correcto y JSON-P Coincide con los anteriores
        // Si devuelven error de invocación que hacemos? TODO:
        // Comprobar governance-version que sea la misma que la nuestra
        // Comprobar si llegamos a Quorum y si es así parar la petición de firmas y empezar a pedir las approves con el evento completo con lo nuevo obtenido en esta fase
        todo!();
    }

    pub fn approver_signatures(&mut self, approval: Approval) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en aprovación o no
        // Comprobar si llegamos a Quorum positivo o negativo
        // Si se llega a Quorum dejamos de pedir approves y empezamos a pedir notarizaciones con el evento completo incluyendo lo nuevo de las approves
        todo!();
    }

    // pub fn notary_signatures(&mut self, ) -> Result<(), EventError> {
    //     // Mirar en que estado está el evento, si está en notarización o no
    //     // Comprobar si llegamos a Quorum y si es así dejar de pedir firmas
    //     // Si se llega a Quorum creamos el evento final, lo firmamos y lo mandamos al ledger
    //     // El ledger se encarga de mandarlo a los testigos o somos nosotros?
    //     todo!();
    // }
}
