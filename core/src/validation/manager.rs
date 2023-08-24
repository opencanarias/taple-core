use super::{
    errors::ValidationError, validation::Validation, ValidationCommand, ValidationResponse,
};
use crate::database::{DatabaseCollection, DB};
use crate::message::MessageTaskCommand;
use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{
    commons::{
        channel::{ChannelData, MpscChannel, SenderEnd},
        self_signature_manager::SelfSignatureManager,
    },
    governance::GovernanceAPI,
};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ValidationAPI {
    sender: SenderEnd<ValidationCommand, ValidationResponse>,
}

#[allow(dead_code)]
impl ValidationAPI {
    pub fn new(sender: SenderEnd<ValidationCommand, ValidationResponse>) -> Self {
        Self { sender }
    }
}

pub struct ValidationManager<C: DatabaseCollection> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<ValidationCommand, ValidationResponse>,
    /// Validation functions
    inner_validation: Validation<C>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<C: DatabaseCollection> ValidationManager<C> {
    pub fn new(
        input_channel: MpscChannel<ValidationCommand, ValidationResponse>,
        gov_api: GovernanceAPI,
        database: DB<C>,
        signature_manager: SelfSignatureManager,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    ) -> Self {
        Self {
            input_channel,
            inner_validation: Validation::new(
                gov_api,
                database,
                signature_manager,
                message_channel,
            ),
            shutdown_receiver,
            shutdown_sender,
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
                            }
                        }
                        None => {
                            self.shutdown_sender.send(()).expect("Channel Closed");
                        },
                    }
                },
                _ = self.shutdown_receiver.recv() => {
                    log::debug!("Validation module shutdown received");
                    break;
                }
            }
        }
    }

    async fn process_command(
        &mut self,
        command: ChannelData<ValidationCommand, ValidationResponse>,
    ) -> Result<(), ValidationError> {
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
                ValidationCommand::ValidationEvent {
                    validation_event,
                    sender,
                } => {
                    let result = self
                        .inner_validation
                        .validation_event(validation_event, sender)
                        .await;
                    match result {
                        Err(ValidationError::ChannelError(_)) => return result.map(|_| ()),
                        _ => ValidationResponse::ValidationEventResponse(result),
                    }
                }
                ValidationCommand::AskForValidation(_) => {
                    log::error!("Ask for Validation in Validation Manager");
                    return Ok(());
                }
            }
        };
        if sender.is_some() {
            sender.unwrap().send(response).expect("Sender Dropped");
        }
        Ok(())
    }
}
