use std::collections::HashSet;

use crate::{
    commons::channel::SenderEnd, database::DB, message::MessageTaskCommand,
    protocol::protocol_message_manager::TapleMessages, DatabaseManager, DigestIdentifier,
    KeyIdentifier,
};

use super::error::AuthorizedSubjectsError;

pub struct AuthorizedSubjects<D: DatabaseManager> {
    database: DB<D>,
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
}

impl<D: DatabaseManager> AuthorizedSubjects<D> {
    pub fn new(
        database: DB<D>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    ) -> Self {
        Self {
            database,
            message_channel,
        }
    }

    pub async fn init(&self) -> Result<(), AuthorizedSubjectsError> {
        Ok(())
    }

    pub async fn new_authorized_subject(
        &self,
        subect_id: DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    ) -> Result<(), AuthorizedSubjectsError> {
        Ok(())
    }
}
