use super::{errors::EventError, event_completer::EventCompleter, EventCommand, EventResponse};
use crate::database::{DatabaseManager, DB};
use crate::governance::error::RequestError;
use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    governance::GovernanceAPI,
    message::MessageTaskCommand,
    protocol::{
        command_head_manager::self_signature_manager::SelfSignatureManager,
        protocol_message_manager::ProtocolManagerMessages,
    },
    Notification,
};

#[derive(Clone, Debug)]
pub struct EventAPI {
    sender: SenderEnd<EventCommand, EventResponse>,
}

impl EventAPI {
    pub fn new(sender: SenderEnd<EventCommand, EventResponse>) -> Self {
        Self { sender }
    }
}

pub struct NotaryManager<D: DatabaseManager> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<EventCommand, EventResponse>,
    /// Notarization functions
    event_completer: EventCompleter<D>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<D: DatabaseManager> NotaryManager<D> {
    pub fn new(
        input_channel: MpscChannel<EventCommand, EventResponse>,
        gov_api: GovernanceAPI,
        database: DB<D>,
        signature_manager: SelfSignatureManager,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        message_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        ledger_sender: SenderEnd<(), ()>,
    ) -> Self {
        Self {
            input_channel,
            event_completer: EventCompleter::new(
                gov_api,
                database,
                signature_manager,
                message_channel,
                notification_sender,
                ledger_sender,
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
                                self.shutdown_sender.send(()).expect("Channel Closed");
                            }
                        }
                        None => {
                            self.shutdown_sender.send(()).expect("Channel Closed");
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
        command: ChannelData<EventCommand, EventResponse>,
    ) -> Result<(), EventError> {
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
                EventCommand::Event {} => {
                    let response = self.event_completer.new_event();
                    match response.clone() {
                        Err(error) => match error {
                            EventError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(EventError::ChannelClosed);
                            }
                            EventError::GovernanceError(inner_error)
                                if inner_error == RequestError::ChannelClosed =>
                            {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(EventError::ChannelClosed);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    EventResponse::Event(response)
                }
                EventCommand::EvaluatorResponse {} => todo!(),
                EventCommand::ApproverResponse {} => todo!(),
                EventCommand::NotaryResponse {} => todo!(),
            }
        };
        if sender.is_some() {
            sender.unwrap().send(response).expect("Sender Dropped");
        }
        Ok(())
    }
}
