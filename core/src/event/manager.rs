use super::{errors::EventError, event_completer::EventCompleter, EventCommand, EventResponse};
use crate::commons::self_signature_manager::SelfSignatureManager;
use crate::database::{DatabaseManager, DB};
use crate::governance::error::RequestError;
use crate::identifier::KeyIdentifier;
use crate::ledger::{LedgerCommand, LedgerResponse};
use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    governance::GovernanceAPI,
    message::MessageTaskCommand,
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

pub struct EventManager<D: DatabaseManager> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<EventCommand, EventResponse>,
    /// Notarization functions
    event_completer: EventCompleter<D>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<D: DatabaseManager> EventManager<D> {
    pub fn new(
        input_channel: MpscChannel<EventCommand, EventResponse>,
        gov_api: GovernanceAPI,
        database: DB<D>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        ledger_sender: SenderEnd<LedgerCommand, LedgerResponse>,
        own_identifier: KeyIdentifier,
        signature_manager: SelfSignatureManager
    ) -> Self {
        Self {
            input_channel,
            event_completer: EventCompleter::new(
                gov_api,
                database,
                message_channel,
                notification_sender,
                ledger_sender,
                own_identifier,
                signature_manager
            ),
            shutdown_receiver,
            shutdown_sender,
        }
    }

    pub async fn start(mut self) {
        match self.event_completer.init().await {
            Ok(_) => {}
            Err(error) => {
                log::error!("Problemas con Init de Event Manager: {:?}", error);
                self.shutdown_sender.send(()).expect("Channel Closed");
                return;
            }
        };
        loop {
            tokio::select! {
                command = self.input_channel.receive() => {
                    match command {
                        Some(command) => {
                            let result = self.process_command(command).await;
                            if result.is_err() {
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
                EventCommand::Event { event_request } => {
                    let response = self.event_completer.new_event(event_request).await;
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
                EventCommand::EvaluatorResponse {
                    evaluation,
                    json_patch,
                    signature,
                } => {
                    match self
                        .event_completer
                        .evaluator_signatures(evaluation, json_patch, signature)
                        .await
                    {
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
                    EventResponse::NoResponse
                }
                EventCommand::ApproverResponse { approval } => {
                    match self.event_completer.approver_signatures(approval).await {
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
                    EventResponse::NoResponse
                }
                EventCommand::ValidatorResponse { signature } => {
                    match self.event_completer.validation_signatures(signature).await {
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
                    EventResponse::NoResponse
                }
                EventCommand::NewGovVersion(new_gov_version) => {
                    match self
                        .event_completer
                        .new_governance_version(
                            new_gov_version.governance_id,
                            new_gov_version.governance_version,
                        )
                        .await
                    {
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
                    };
                    EventResponse::NoResponse
                }
            }
        };
        if sender.is_some() {
            sender.unwrap().send(response).expect("Sender Dropped");
        }
        Ok(())
    }
}
