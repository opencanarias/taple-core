use std::collections::{HashMap, HashSet};

use crate::{
    commons::{channel::SenderEnd, errors::ChannelErrors},
    governance::{GovernanceAPI, GovernanceInterface},
    identifier::DigestIdentifier,
    message::MessageTaskCommand,
    protocol::{
        command_head_manager::self_signature_manager::{
            SelfSignatureInterface, SelfSignatureManager,
        },
        protocol_message_manager::ProtocolManagerMessages,
    }, Notification,
};

use super::{errors::EventError, EventCommand, EventResponse, EventMessages};
use crate::database::{DatabaseManager, DB};

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

    pub fn new_event(&mut self, ) -> Result<DigestIdentifier, EventError> {
        // Comprobar si ya tenemos un evento para ese sujeto
        // Comprobar si el contenido es correcto (firma, invokator, etc)
        // Pedir firmas de evaluación, mandando request, sn y firma del sujeto de todo
        todo!();
    }

    pub fn evaluator_signatures(&mut self, ) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en evaluación o no
        // Comprobar si la versión de la governanza coincide con la nuestra, si no no lo aceptamos
        // Comprobar que todo es correcto y JSON-P Coincide con los anteriores
        // Si devuelven error de invocación que hacemos? TODO:
        // Comprobar governance-version que sea la misma que la nuestra
        // Comprobar si llegamos a Quorum y si es así parar la petición de firmas y empezar a pedir las approves con el evento completo con lo nuevo obtenido en esta fase
        todo!();
    }

    pub fn approver_signatures(&mut self, ) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en aprovación o no
        // Comprobar si llegamos a Quorum positivo o negativo
        // Si se llega a Quorum dejamos de pedir approves y empezamos a pedir notarizaciones con el evento completo incluyendo lo nuevo de las approves
        todo!();
    }

    pub fn notary_signatures(&mut self, ) -> Result<(), EventError> {
        // Mirar en que estado está el evento, si está en notarización o no
        // Comprobar si llegamos a Quorum y si es así dejar de pedir firmas
        // Si se llega a Quorum creamos el evento final, lo firmamos y lo mandamos al ledger
        // El ledger se encarga de mandarlo a los testigos o somos nosotros?
        todo!();
    }
}
