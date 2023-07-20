use std::collections::HashSet;

use crate::{
    commons::channel::SenderEnd,
    database::DB,
    ledger::LedgerCommand,
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
    DatabaseCollection, DigestIdentifier, KeyIdentifier,
};

use super::error::AuthorizedSubjectsError;

/// Structure that manages the pre-authorized subjects in a system and communicates with other components of the system through a message channel.
pub struct AuthorizedSubjects<C: DatabaseCollection> {
    /// Object that handles the connection to the database.
    database: DB<C>,
    /// Message channel used to communicate with other system components.
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    /// Unique identifier for the component using this structure.
    our_id: KeyIdentifier,
}

impl<C: DatabaseCollection> AuthorizedSubjects<C> {
    /// Creates a new instance of the `AuthorizedSubjects` structure.
    ///
    /// # Arguments
    ///
    /// * `database` - Database connection.
    /// * `message_channel` - Message channel.
    /// * `our_id` - Unique identifier.
    pub fn new(
        database: DB<C>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        our_id: KeyIdentifier,
    ) -> Self {
        Self {
            database,
            message_channel,
            our_id,
        }
    }

    /// Obtains all pre-authorized subjects and sends a message to the associated providers through the message channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the preauthorized subjects cannot be obtained or if a message cannot be sent through the message channel.
    pub async fn ask_for_all(&self) -> Result<(), AuthorizedSubjectsError> {
        // We obtain all pre-authorized subjects from the database.
        let preauthorized_subjects = match self
            .database
            .get_allowed_subjects_and_providers(None, 10000)
        {
            Ok(psp) => psp,
            Err(error) => match error {
                _ => return Err(AuthorizedSubjectsError::DatabaseError(error)),
            },
        };

        // For each pre-authorized subject, we send a message to the associated providers through the message channel.
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

    /// Add a new pre-authorized subject and send a message to the associated suppliers through the message channel.
    ///
    /// # Arguments
    ///
    /// * `subject_id` - Identifier of the new pre-authorized subject.
    /// * `providers` - Set of associated provider identifiers.
    ///
    /// # Errors
    ///
    /// Returns an error if a message cannot be sent through the message channel.
    pub async fn new_authorized_subject(
        &self,
        subject_id: DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    ) -> Result<(), AuthorizedSubjectsError> {
        self.database
            .set_preauthorized_subject_and_providers(&subject_id, providers.clone())?;
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
