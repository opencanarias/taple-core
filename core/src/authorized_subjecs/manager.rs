use std::collections::HashSet;

use tokio::time::{interval, Duration};

use crate::database::Error as DbError;
use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    database::DB,
    message::MessageTaskCommand,
    protocol::protocol_message_manager::TapleMessages,
    DatabaseCollection, DigestIdentifier, KeyIdentifier,
};

use super::{
    authorized_subjects::AuthorizedSubjects, error::AuthorizedSubjectsError,
    AuthorizedSubjectsCommand, AuthorizedSubjectsResponse,
};

#[derive(Clone, Debug)]
pub struct AuthorizedSubjectsAPI {
    sender: SenderEnd<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>,
}

impl AuthorizedSubjectsAPI {
    pub fn new(sender: SenderEnd<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>) -> Self {
        Self { sender }
    }

    pub async fn new_authorized_subject(
        &self,
        subject_id: DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    ) -> Result<(), AuthorizedSubjectsError> {
        self.sender
            .tell(AuthorizedSubjectsCommand::NewAuthorizedSubject {
                subject_id,
                providers,
            })
            .await?;
        Ok(())
    }
}

/// Manages authorized subjects and their providers.
pub struct AuthorizedSubjectsManager<C: DatabaseCollection> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>,
    inner_authorized_subjects: AuthorizedSubjects<C>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<C: DatabaseCollection> AuthorizedSubjectsManager<C> {
    /// Creates a new `AuthorizedSubjectsManager` with the given input channel, database, message channel, ID, and shutdown channels.
    pub fn new(
        input_channel: MpscChannel<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>,
        database: DB<C>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        our_id: KeyIdentifier,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        Self {
            input_channel,
            inner_authorized_subjects: AuthorizedSubjects::new(database, message_channel, our_id),
            shutdown_sender,
            shutdown_receiver,
        }
    }

    /// Starts the `AuthorizedSubjectsManager` and processes incoming commands.
    pub async fn start(mut self) {
        // Ask for all authorized subjects from the database
        match self.inner_authorized_subjects.ask_for_all().await {
            Ok(_) => {}
            Err(AuthorizedSubjectsError::DatabaseError(DbError::EntryNotFound)) => {}
            Err(error) => {
                log::error!("{}", error);
                self.shutdown_sender.send(()).expect("Channel Closed");
                return;
            }
        };
        // Set up a timer to periodically ask for all authorized subjects from the database
        let mut timer = interval(Duration::from_secs(15));
        loop {
            tokio::select! {
                // Process incoming commands from the input channel
                command = self.input_channel.receive() => {
                    match command {
                        Some(command) => {
                            let result = self.process_command(command).await;
                            if result.is_err() {
                                log::error!("{}", result.unwrap_err());
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                break;
                            }
                        }
                        None => {
                            self.shutdown_sender.send(()).expect("Channel Closed");
                            break;
                        },
                    }
                },
                // Ask for all authorized subjects from the database when the timer ticks
                _ = timer.tick() => {
                    match self.inner_authorized_subjects.ask_for_all().await {
                        Ok(_) => {}
                        Err(AuthorizedSubjectsError::DatabaseError(DbError::EntryNotFound)) => {}
                        Err(error) => {
                            log::error!("{}", error);
                            self.shutdown_sender.send(()).expect("Channel Closed");
                            break;
                        }
                    };
                },
                // Shutdown the manager when a shutdown signal is received
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }

    /// Processes an incoming command from the input channel.
    async fn process_command(
        &mut self,
        command: ChannelData<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>,
    ) -> Result<(), AuthorizedSubjectsError> {
        let (sender, data) = match command {
            ChannelData::AskData(data) => {
                let (sender, data) = data.get();
                (Some(sender), data)
            }
            ChannelData::TellData(data) => {
                let data = data.get();
                (None, data)
            }
        };
        let response = {
            match data {
                AuthorizedSubjectsCommand::NewAuthorizedSubject {
                    subject_id,
                    providers,
                } => {
                    let response = self
                        .inner_authorized_subjects
                        .new_authorized_subject(subject_id, providers)
                        .await;
                    match response {
                        Ok(_) => {}
                        Err(error) => match error {
                            AuthorizedSubjectsError::DatabaseError(db_error) => match db_error {
                                crate::DbError::EntryNotFound => todo!(),
                                _ => return Err(AuthorizedSubjectsError::DatabaseError(db_error)),
                            },
                            _ => return Err(error),
                        },
                    }
                    AuthorizedSubjectsResponse::NoResponse
                }
            }
        };
        if sender.is_some() {
            sender.unwrap().send(response).expect("Sender Dropped");
        }
        Ok(())
    }
}
