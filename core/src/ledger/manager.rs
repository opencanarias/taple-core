use async_trait::async_trait;

use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    database::DB,
    distribution::{error::DistributionErrorResponses, DistributionMessagesNew},
    governance::{error::RequestError, GovernanceAPI},
    message::MessageTaskCommand,
    protocol::protocol_message_manager::TapleMessages,
    DatabaseCollection, DigestIdentifier, KeyIdentifier, Notification,
};

use super::{errors::LedgerError, ledger::Ledger, LedgerCommand, LedgerResponse};

#[async_trait]
pub trait EventManagerInterface {
    async fn expecting_transfer(
        &self,
        subject_id: DigestIdentifier,
    ) -> Result<KeyIdentifier, LedgerError>;
}

#[derive(Debug, Clone)]
pub struct EventManagerAPI {
    sender: SenderEnd<LedgerCommand, LedgerResponse>,
}

impl EventManagerAPI {
    pub fn new(sender: SenderEnd<LedgerCommand, LedgerResponse>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl EventManagerInterface for EventManagerAPI {
    async fn expecting_transfer(
        &self,
        subject_id: DigestIdentifier,
    ) -> Result<KeyIdentifier, LedgerError> {
        let response = self
            .sender
            .ask(LedgerCommand::ExpectingTransfer { subject_id })
            .await
            .map_err(|_| LedgerError::ChannelClosed)?;
        if let LedgerResponse::ExpectingTransfer(public_key) = response {
            public_key
        } else {
            Err(LedgerError::UnexpectedResponse)
        }
    }
}

pub struct EventManager<C: DatabaseCollection> {
    /// Communication channel for incoming petitions
    input_channel: MpscChannel<LedgerCommand, LedgerResponse>,
    inner_ledger: Ledger<C>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    notification_sender: tokio::sync::broadcast::Sender<Notification>,
}

impl<C: DatabaseCollection> EventManager<C> {
    pub fn new(
        input_channel: MpscChannel<LedgerCommand, LedgerResponse>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        gov_api: GovernanceAPI,
        database: DB<C>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        distribution_channel: SenderEnd<
            DistributionMessagesNew,
            Result<(), DistributionErrorResponses>,
        >,
        our_id: KeyIdentifier,
    ) -> Self {
        Self {
            input_channel,
            inner_ledger: Ledger::new(
                gov_api,
                database,
                message_channel,
                distribution_channel,
                our_id,
            ),
            shutdown_receiver,
            shutdown_sender,
            notification_sender,
        }
    }

    pub async fn start(mut self) {
        match self.inner_ledger.init().await {
            Ok(_) => {}
            Err(error) => {
                log::error!("Ledger Manager Init fails: {:?}", error);
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
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }

    async fn process_command(
        &mut self,
        command: ChannelData<LedgerCommand, LedgerResponse>,
    ) -> Result<(), LedgerError> {
        log::warn!("MENSAJE EN EL LEDGER RECIBIDO");
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
                LedgerCommand::ExpectingTransfer { subject_id } => {
                    let response = self.inner_ledger.expecting_transfer(subject_id).await;
                    match &response {
                        Err(error) => match error {
                            LedgerError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            LedgerError::GovernanceError(inner_error)
                                if *inner_error == RequestError::ChannelClosed =>
                            {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    LedgerResponse::ExpectingTransfer(response)
                }
                LedgerCommand::OwnEvent { event, signatures, validation_proof } => {
                    let response = self.inner_ledger.event_validated(event, signatures, validation_proof).await;
                    match response {
                        Err(error) => match error {
                            LedgerError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            LedgerError::GovernanceError(inner_error)
                                if inner_error == RequestError::ChannelClosed =>
                            {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            _ => {
                                log::error!("ERROR EN LEDGER {}", error);
                            }
                        },
                        _ => {}
                    }
                    LedgerResponse::NoResponse
                }
                LedgerCommand::Genesis { event_request } => {
                    let response = self.inner_ledger.genesis(event_request).await;
                    match response {
                        Err(error) => match error {
                            LedgerError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            LedgerError::GovernanceError(inner_error)
                                if inner_error == RequestError::ChannelClosed =>
                            {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    LedgerResponse::NoResponse
                }
                LedgerCommand::ExternalEvent {
                    sender,
                    event,
                    signatures,
                    validation_proof,
                } => {
                    log::error!("EXTERNAL EVENT RECIVED");
                    let response = self
                        .inner_ledger
                        .external_event(event, signatures, sender, validation_proof)
                        .await;
                    log::error!("External Event Response: {:?}", response);
                    match response {
                        Err(error) => match error {
                            LedgerError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            LedgerError::GovernanceError(inner_error)
                                if inner_error == RequestError::ChannelClosed =>
                            {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    LedgerResponse::NoResponse
                }
                LedgerCommand::ExternalIntermediateEvent { event } => {
                    log::error!("EXTERNAL INTERMEDIATE EVENT");
                    let response = self.inner_ledger.external_intermediate_event(event).await;
                    match response {
                        Err(error) => match error {
                            LedgerError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            LedgerError::GovernanceError(inner_error)
                                if inner_error == RequestError::ChannelClosed =>
                            {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    LedgerResponse::NoResponse
                }
                LedgerCommand::GetEvent {
                    who_asked,
                    subject_id,
                    sn,
                } => {
                    let response = self.inner_ledger.get_event(who_asked, subject_id, sn).await;
                    let response = match response {
                        Err(error) => match error.clone() {
                            LedgerError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            LedgerError::DatabaseError(err) => match err {
                                crate::DbError::EntryNotFound => return Ok(()),
                                _ => Err(error),
                            },
                            _ => Err(error),
                        },
                        Ok(event) => Ok(event),
                    };
                    LedgerResponse::GetEvent(response)
                }
                LedgerCommand::GetLCE {
                    who_asked,
                    subject_id,
                } => {
                    let response = self.inner_ledger.get_lce(who_asked, subject_id).await;
                    let response = match response {
                        Err(error) => match error.clone() {
                            LedgerError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            LedgerError::DatabaseError(err) => match err {
                                crate::DbError::EntryNotFound => return Ok(()),
                                _ => Err(error),
                            },
                            _ => Err(error),
                        },
                        Ok(event) => Ok(event),
                    };
                    log::error!("GETLCE RESPONSE OK");
                    LedgerResponse::GetLCE(response)
                }
                LedgerCommand::GetNextGov {
                    who_asked,
                    subject_id,
                    sn,
                } => {
                    let response = self
                        .inner_ledger
                        .get_next_gov(who_asked, subject_id, sn)
                        .await;
                    let response = match response {
                        Err(error) => match error.clone() {
                            LedgerError::ChannelClosed => {
                                log::error!("Channel Closed");
                                self.shutdown_sender.send(()).expect("Channel Closed");
                                return Err(LedgerError::ChannelClosed);
                            }
                            LedgerError::DatabaseError(err) => match err {
                                crate::DbError::EntryNotFound => return Ok(()),
                                _ => Err(error),
                            },
                            _ => Err(error),
                        },
                        Ok(event) => Ok(event),
                    };
                    LedgerResponse::GetNextGov(response)
                }
            }
        };
        if sender.is_some() {
            sender.unwrap().send(response).expect("Sender Dropped");
        }
        Ok(())
    }
}
