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

use super::{errors::EventError, EventCommand, EventResponse};
use crate::database::{DatabaseManager, DB};

pub struct EventCompleter<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    signature_manager: SelfSignatureManager,
    message_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
    notification_sender: tokio::sync::broadcast::Sender<Notification>,
    ledger_sender: SenderEnd<(), ()>,
    subjects_completing_event: HashSet<DigestIdentifier>,
}

impl<D: DatabaseManager> EventCompleter<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<D>,
        signature_manager: SelfSignatureManager,
        message_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
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
        todo!();
    }

    pub fn evaluator_signatures(&mut self, ) -> Result<(), EventError> {
        todo!();
    }

    pub fn approver_signatures(&mut self, ) -> Result<(), EventError> {
        todo!();
    }

    pub fn notary_signatures(&mut self, ) -> Result<(), EventError> {
        todo!();
    }
}
