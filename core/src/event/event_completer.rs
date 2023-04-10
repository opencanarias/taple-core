use std::collections::HashMap;

use crate::{
    commons::errors::ChannelErrors,
    governance::{GovernanceAPI, GovernanceInterface},
    identifier::DigestIdentifier,
    protocol::command_head_manager::self_signature_manager::{
        SelfSignatureInterface, SelfSignatureManager,
    },
};

use super::{errors::EventError, EventCommand, EventResponse};
use crate::database::{DatabaseManager, DB};

pub struct EventCompleter<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    signature_manager: SelfSignatureManager,
}

impl<D: DatabaseManager> EventCompleter<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<D>,
        signature_manager: SelfSignatureManager,
    ) -> Self {
        Self {
            gov_api,
            database,
            signature_manager,
        }
    }
}