use async_trait::async_trait;

use super::{errors::EventError, event_completer::EventCompleter, EventCommand, EventResponse};
use crate::EventRequest;
use crate::commons::self_signature_manager::SelfSignatureManager;
use crate::database::{DB, DatabaseCollection};
use crate::governance::error::RequestError;
use crate::governance::GovernanceUpdatedMessage;
use crate::identifier::KeyIdentifier;
use crate::ledger::{LedgerCommand, LedgerResponse};
use crate::protocol::protocol_message_manager::TapleMessages;
use crate::signature::Signed;
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

#[async_trait]
pub trait EventAPIInterface {
    async fn send_event_request(&self, event_request: Signed<EventRequest>) -> EventResponse;
}

#[async_trait]
impl EventAPIInterface for EventAPI {
    async fn send_event_request(&self, event_request: Signed<EventRequest>) -> EventResponse {
        match self.sender.ask(EventCommand::Event { event_request }).await {
            Ok(response) => response,
            Err(_) => EventResponse::Event(Err(EventError::EventApiChannelNotAvailable)),
        }
    }
}

pub struct EventManager<C: DatabaseCollection> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<EventCommand, EventResponse>,
    input_channel_updated_gov: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
    /// Notarization functions
    event_completer: EventCompleter<C>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<C: DatabaseCollection> EventManager<C> {
    pub fn new(
        input_channel: MpscChannel<EventCommand, EventResponse>,
        input_channel_updated_gov: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
        gov_api: GovernanceAPI,
        database: DB<C>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        ledger_sender: SenderEnd<LedgerCommand, LedgerResponse>,
        own_identifier: KeyIdentifier,
        signature_manager: SelfSignatureManager,
    ) -> Self {
        Self {
            input_channel,
            input_channel_updated_gov,
            event_completer: EventCompleter::new(
                gov_api,
                database,
                message_channel,
                notification_sender,
                ledger_sender,
                own_identifier,
                signature_manager,
            ),
            shutdown_receiver,
            shutdown_sender,
        }
    }

    pub async fn start(mut self) {
        match self.event_completer.init().await {
            Ok(_) => {}
            Err(error) => {
                log::error!("{}", error);
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
                gov_updated = self.input_channel_updated_gov.recv() => {
                    match gov_updated {
                        Ok(gov_updated) => {
                            match gov_updated {
                                GovernanceUpdatedMessage::GovernanceUpdated { governance_id, governance_version } => {
                                    let result = self.event_completer.new_governance_version(governance_id, governance_version).await;
                                    if result.is_err() {
                                        self.shutdown_sender.send(()).expect("Channel Closed");
                                        break;
                                    }
                                },
                            }
                        },
                        Err(_) => {
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
                    let response = self.event_completer.pre_new_event(event_request).await;
                    log::warn!("EVENT REPSONE: {:?}", response);
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
                    evaluator_response,
                } => {
                    match self
                        .event_completer
                        .evaluator_signatures(evaluator_response)
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
                            _ => {
                                log::error!("{:?}", error);
                            }
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
                EventCommand::ValidatorResponse {
                    event_hash,
                    signature,
                    governance_version,
                } => {
                    match self
                        .event_completer
                        .validation_signatures(event_hash, signature, governance_version)
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
                            _ => {
                                log::error!("VALIDATION ERROR: {:?}", error);
                            }
                        },
                        _ => {
                            log::error!("VALIDATION ACCEPTED");
                        }
                    }
                    EventResponse::NoResponse
                }
                EventCommand::HigherGovernanceExpected {
                    governance_id,
                    who_asked,
                } => {
                    match self
                        .event_completer
                        .higher_governance_expected(governance_id, who_asked)
                        .await
                    {
                        Ok(_) => {}
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
                            _ => {
                                log::error!("VALIDATION ERROR: {:?}", error);
                            }
                        },
                    }
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
