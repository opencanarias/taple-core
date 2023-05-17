use std::collections::HashSet;

use crate::{
    commons::channel::SenderEnd,
    database::DB,
    distribution::LedgerMessages,
    ledger::LedgerCommand,
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
    DatabaseManager, DigestIdentifier, KeyIdentifier,
};

use super::error::AuthorizedSubjectsError;

pub struct AuthorizedSubjects<D: DatabaseManager> {
    database: DB<D>,
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    our_id: KeyIdentifier,
}

impl<D: DatabaseManager> AuthorizedSubjects<D> {
    pub fn new(
        database: DB<D>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        our_id: KeyIdentifier,
    ) -> Self {
        Self {
            database,
            message_channel,
            our_id,
        }
    }

    pub async fn ask_for_all(&self) -> Result<(), AuthorizedSubjectsError> {
        let preauthorized_subjects = self
            .database
            .get_preauthorized_subjects_and_providers(None, 10000)?;
        for (subject_id, providers) in preauthorized_subjects.into_iter() {
            if !providers.is_empty() {
                self.message_channel
                    .tell(MessageTaskCommand::Request(
                        None,
                        TapleMessages::LedgerMessages(LedgerCommand::GetLCE {
                            who_asked: self.our_id.clone(),
                            subject_id,
                        }),
                        providers.into_iter().collect(),
                        MessageConfig::direct_response(),
                    ))
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn new_authorized_subject(
        &self,
        subject_id: DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    ) -> Result<(), AuthorizedSubjectsError> {
        if !providers.is_empty() {
            self.message_channel
                .tell(MessageTaskCommand::Request(
                    None,
                    TapleMessages::LedgerMessages(LedgerCommand::GetLCE {
                        who_asked: self.our_id.clone(),
                        subject_id,
                    }),
                    providers.into_iter().collect(),
                    MessageConfig::direct_response(),
                ))
                .await?;
        }
        Ok(())
    }
}
