use std::collections::HashMap;

use crate::{
    commons::{errors::ChannelErrors, channel::SenderEnd},
    governance::{GovernanceAPI, GovernanceInterface},
    identifier::DigestIdentifier,
    protocol::{command_head_manager::self_signature_manager::{
        SelfSignatureInterface, SelfSignatureManager,
    }, protocol_message_manager::ProtocolManagerMessages}, message::MessageTaskCommand,
};

use super::{errors::EventError, EventCommand, EventResponse};
use crate::database::{DatabaseManager, DB};

pub struct EventCompleter<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    signature_manager: SelfSignatureManager,
    message_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
}

impl<D: DatabaseManager> EventCompleter<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<D>,
        signature_manager: SelfSignatureManager,
        message_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
    ) -> Self {
        Self {
            gov_api,
            database,
            signature_manager,
            message_channel,
        }
    }
}
