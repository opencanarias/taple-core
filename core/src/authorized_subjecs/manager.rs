use crate::{
    commons::channel::{ChannelData, MpscChannel},
    database::DB,
    DatabaseManager,
};

use super::{
    error::AuthorizedSubjectsError, AuthorizedSubjectsCommand, AuthorizedSubjectsResponse, authorized_subjects::AuthorizedSubjects,
};

pub struct AuthorizedSubjectsManager<D: DatabaseManager> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>,
    inner_authorized_subjects: AuthorizedSubjects<D>,
    /// Notarization functions
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<D: DatabaseManager> AuthorizedSubjectsManager<D> {
    pub fn new(
        input_channel: MpscChannel<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>,
        database: DB<D>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        Self {
            input_channel,
            inner_authorized_subjects: AuthorizedSubjects::new(database),
            shutdown_sender,
            shutdown_receiver,
        }
    }

    pub async fn start(mut self) {
        loop {
            tokio::select! {
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
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }

    async fn process_command(
        &mut self,
        command: ChannelData<AuthorizedSubjectsCommand, AuthorizedSubjectsResponse>,
    ) -> Result<(), AuthorizedSubjectsError> {
        log::info!("EVENT MANAGER MSG RECEIVED");
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
                AuthorizedSubjectsCommand::NewAuthorizedGovernance {
                    subject_id,
                    providers,
                } => todo!(),
            }
        };
        if sender.is_some() {
            sender.unwrap().send(response).expect("Sender Dropped");
        }
        Ok(())
    }
}
