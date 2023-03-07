use async_trait::async_trait;
use crate::commons::{
    channel::{ChannelData, MpscChannel, SenderEnd},
    config::TapleSettings,
    crypto::KeyPair,
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{
        event::Event,
        event_request::{
            EventRequest, EventRequestType,
        },
        notification::Notification,
        signature::Signature,
        state::SubjectData,
    },
};
use crate::governance::{GovernanceAPI, GovernanceMessage, GovernanceResponse};
use crate::ledger::{
    errors::LedgerManagerError,
    ledger_manager::{CommandManagerMessage, CommandManagerResponse as LedgerResponse, LedgerAPI},
};
use crate::message::MessageTaskCommand;
use serde_json::Value;
use std::collections::HashSet;
use tokio::sync::watch::Receiver as WatchReceiver;

use super::super::{
    errors::{ProtocolErrors, ResponseError},
    protocol_message_manager::ProtocolManagerMessages,
};

use super::{
    inner_manager::{InnerManager, NotifierInterface},
    self_signature_manager::SelfSignatureManager,
    CommandGetEventResponse, CommandGetResponse, CommandGetSignaturesResponse,
    CommandManagerResponses, CommandSendResponse, Commands, Conflict, Content, CreateEventResponse,
    GetMessage, SendResponse,
};

#[async_trait]
pub trait CommandManagerInterface {
    async fn get_event(
        &self,
        sn: u64,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<Event, ResponseError>;
    async fn get_head(
        &self,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<Event, ResponseError>;
    async fn set_event(&self, event: Event) -> Result<(), ResponseError>;
    async fn get_signatures(
        &self,
        sn: u64,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<HashSet<Signature>, ResponseError>;
    async fn set_signatures(
        &self,
        sn: u64,
        subject_id: DigestIdentifier,
        signatures: HashSet<Signature>,
    ) -> Result<(), ResponseError>;
    async fn get_lce(
        &self,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<(Event, HashSet<Signature>), ResponseError>;
    async fn set_lce(
        &self,
        event: Event,
        signatures: HashSet<Signature>,
    ) -> Result<(), ResponseError>;
    async fn get_subject(&self, subject_id: DigestIdentifier)
        -> Result<SubjectData, ResponseError>;
    async fn get_subjects(&self, namespace: String) -> Result<Vec<SubjectData>, ResponseError>;
    async fn get_schema(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
    ) -> Result<Value, ResponseError>;
    async fn create_event(
        &self,
        request: EventRequest,
        approved: bool,
    ) -> Result<Event, ResponseError>;
}

pub struct CommandAPI {
    sender: SenderEnd<Commands, CommandManagerResponses>,
}

impl CommandAPI {
    pub fn new(sender: SenderEnd<Commands, CommandManagerResponses>) -> Self {
        Self { sender }
    }

    async fn set_general<F>(
        &self,
        event: Option<Event>,
        signatures: Option<HashSet<Signature>>,
        sn: u64,
        subject_id: DigestIdentifier,
        handler: F,
    ) -> Result<(), ResponseError>
    where
        F: Fn(&CommandSendResponse) -> Result<(), ResponseError>,
    {
        let response = self
            .sender
            .ask(Commands::SendMessage(super::SendMessage {
                event,
                signatures,
                sn,
                subject_id,
            }))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        let CommandManagerResponses::SendResponse(data) = response else {
            return Err(ResponseError::UnexpectedCommandResponse);
        };
        handler(&data)
    }

    async fn get_general<Output, F>(
        &self,
        sn: super::EventId,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
        content: HashSet<Content>,
        handler: F,
    ) -> Result<Output, ResponseError>
    where
        F: Fn(CommandGetResponse) -> Result<Output, ResponseError>,
    {
        let response = self
            .sender
            .ask(Commands::GetMessage(GetMessage {
                sn,
                subject_id,
                sender_id,
                request_content: content,
            }))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        let CommandManagerResponses::GetResponse(data) = response else {
            return Err(ResponseError::UnexpectedCommandResponse);
        };
        handler(data)
    }

    fn get_event_general(data: CommandGetResponse) -> Result<Event, ResponseError> {
        match data.event {
            Some(CommandGetEventResponse::Data(event)) => Ok(event),
            Some(CommandGetEventResponse::Conflict(Conflict::EventNotFound)) => {
                Err(ResponseError::EventNotFound)
            }
            Some(CommandGetEventResponse::Conflict(Conflict::SubjectNotFound)) => {
                Err(ResponseError::SubjectNotFound)
            }
            None => Err(ResponseError::UnexpectedCommandResponse),
        }
    }

    fn get_signature_general(
        data: CommandGetResponse,
    ) -> Result<HashSet<Signature>, ResponseError> {
        match data.signatures {
            Some(CommandGetSignaturesResponse::Data(signatures)) => Ok(signatures),
            Some(CommandGetSignaturesResponse::Conflict(Conflict::EventNotFound)) => {
                Err(ResponseError::EventNotFound)
            }
            Some(CommandGetSignaturesResponse::Conflict(Conflict::SubjectNotFound)) => {
                Err(ResponseError::SubjectNotFound)
            }
            None => Err(ResponseError::UnexpectedCommandResponse),
        }
    }

    fn set_event_general(data: &CommandSendResponse) -> Result<(), ResponseError> {
        match data.event.as_ref() {
            Some(data) => match data {
                SendResponse::Valid => Ok(()),
                SendResponse::Invalid => Err(ResponseError::InvalidSetOperation),
            },
            None => Err(ResponseError::UnexpectedCommandResponse),
        }
    }

    fn set_signatures_general(data: &CommandSendResponse) -> Result<(), ResponseError> {
        match data.signatures.as_ref() {
            Some(data) => match data {
                SendResponse::Valid => Ok(()),
                SendResponse::Invalid => Err(ResponseError::InvalidSetOperation),
            },
            None => Err(ResponseError::UnexpectedCommandResponse),
        }
    }
}

#[async_trait]
impl CommandManagerInterface for CommandAPI {
    async fn get_event(
        &self,
        sn: u64,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<Event, ResponseError> {
        self.get_general(
            super::EventId::SN { sn },
            subject_id,
            sender_id,
            HashSet::from_iter(vec![Content::Event]),
            Self::get_event_general,
        )
        .await
    }

    async fn get_head(
        &self,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<Event, ResponseError> {
        self.get_general(
            super::EventId::HEAD,
            subject_id,
            sender_id,
            HashSet::from_iter(vec![Content::Event]),
            Self::get_event_general,
        )
        .await
    }

    async fn set_event(&self, event: Event) -> Result<(), ResponseError> {
        let sn = event.event_content.sn;
        let subject_id = event.event_content.subject_id.clone();
        self.set_general(Some(event), None, sn, subject_id, Self::set_event_general)
            .await
    }

    async fn get_signatures(
        &self,
        sn: u64,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<HashSet<Signature>, ResponseError> {
        self.get_general(
            super::EventId::SN { sn },
            subject_id,
            sender_id,
            HashSet::from_iter(vec![Content::Signatures(HashSet::new())]),
            Self::get_signature_general,
        )
        .await
    }
    async fn set_signatures(
        &self,
        sn: u64,
        subject_id: DigestIdentifier,
        signatures: HashSet<Signature>,
    ) -> Result<(), ResponseError> {
        self.set_general(
            None,
            Some(signatures),
            sn,
            subject_id,
            Self::set_signatures_general,
        )
        .await
    }
    async fn get_lce(
        &self,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<(Event, HashSet<Signature>), ResponseError> {
        self.get_general(
            super::EventId::HEAD,
            subject_id,
            sender_id,
            HashSet::from_iter(vec![Content::Event, Content::Signatures(HashSet::new())]),
            |data| {
                let event = Self::get_event_general(data.clone())?;
                let signatures = Self::get_signature_general(data)?;
                Ok((event, signatures))
            },
        )
        .await
    }
    async fn set_lce(
        &self,
        event: Event,
        signatures: HashSet<Signature>,
    ) -> Result<(), ResponseError> {
        let sn = event.event_content.sn;
        let subject_id = event.event_content.subject_id.clone();
        self.set_general(Some(event), Some(signatures), sn, subject_id, |data| {
            Self::set_event_general(data)?;
            Self::set_signatures_general(data)?;
            Ok(())
        })
        .await
    }
    async fn get_subject(
        &self,
        subject_id: DigestIdentifier,
    ) -> Result<SubjectData, ResponseError> {
        let response = self
            .sender
            .ask(Commands::GetSingleSubject(super::GetSingleSubject {
                subject_id,
            }))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        let CommandManagerResponses::GetSingleSubjectResponse(data) = response else {
            return Err(ResponseError::UnexpectedCommandResponse);
        };
        data
    }
    async fn get_subjects(&self, namespace: String) -> Result<Vec<SubjectData>, ResponseError> {
        let response = self
            .sender
            .ask(Commands::GetSubjects(super::GetSubjects { namespace }))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        let CommandManagerResponses::GetSubjectsResponse(data) = response else {
            return Err(ResponseError::UnexpectedCommandResponse);
        };
        data
    }
    async fn get_schema(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
    ) -> Result<Value, ResponseError> {
        let response = self
            .sender
            .ask(Commands::GetSchema(super::GetSchema {
                governance_id,
                schema_id,
            }))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        let CommandManagerResponses::GetSchema(data) = response else {
                return Err(ResponseError::UnexpectedCommandResponse);
            };
        data
    }
    async fn create_event(
        &self,
        request: EventRequest,
        approved: bool,
    ) -> Result<Event, ResponseError> {
        if let EventRequestType::Create(data) = &request.request {
             // Check if data correspond to a new governance
             if data.governance_id.digest.is_empty() {
                // It can only by a governance
                if data.schema_id != "governance" {
                    return Err(ResponseError::CantCreateGovernance);
                }
            } else if data.schema_id == "governance" {
                return Err(ResponseError::CantCreateGovernance);
            }
        };
        let response = self.sender
            .ask(Commands::CreateEventMessage(request, approved))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        let CommandManagerResponses::CreateEventResponse(data) = response else {
            return Err(ResponseError::UnexpectedCommandResponse);
        };
        match data {
            CreateEventResponse::Event(event) => Ok(event),
            CreateEventResponse::Error(error) => Err(error),
        }
    }
    // async fn create_event(
    //     &self,
    //     subject_id: DigestIdentifier,
    //     payload: RequestPayload,
    //     request_signature: Signature,
    //     approvals: HashSet<ApprovalResponse>,
    //     timestamp: Option<i64>,
    //     approved: bool,
    // ) -> Result<Event, ResponseError> {
    //     let response = self
    //         .sender
    //         .ask(Commands::CreateEventMessage(
    //             EventRequest {
    //                 request: EventRequestType::State(StateRequest {
    //                     subject_id,
    //                     payload,
    //                 }),
    //                 signature: request_signature,
    //                 timestamp: if timestamp.is_none() {
    //                     Utc::now().timestamp_millis()
    //                 } else {
    //                     timestamp.unwrap()
    //                 },
    //                 approvals,
    //             },
    //             approved,
    //         ))
    //         .await
    //         .map_err(|_| ResponseError::ComunnicationClosed)?;
    //     let CommandManagerResponses::CreateEventResponse(data) = response else {
    //         return Err(ResponseError::UnexpectedCommandResponse);
    //     };
    //     match data {
    //         CreateEventResponse::Event(event) => Ok(event),
    //         CreateEventResponse::Error(error) => Err(error),
    //     }
    // }
    // async fn create_subject(
    //     &self,
    //     governance_id: DigestIdentifier,
    //     schema_id: String,
    //     namespace: String,
    //     payload: RequestPayload,
    //     request_signature: Signature,
    //     approvals: HashSet<ApprovalResponse>,
    //     timestamp: Option<i64>,
    // ) -> Result<Event, ResponseError> {
    //     let response = self
    //         .sender
    //         .ask(Commands::CreateEventMessage(
    //             EventRequest {
    //                 request: EventRequestType::Create(CreateRequest {
    //                     governance_id,
    //                     schema_id,
    //                     namespace,
    //                     payload,
    //                 }),
    //                 signature: request_signature,
    //                 timestamp: if timestamp.is_none() {
    //                     Utc::now().timestamp_millis()
    //                 } else {
    //                     timestamp.unwrap()
    //                 },
    //                 approvals,
    //             },
    //             true, // TODO: For now we always approve subject creation events
    //         ))
    //         .await
    //         .map_err(|_| ResponseError::ComunnicationClosed)?;
    //     let CommandManagerResponses::CreateEventResponse(data) = response else {
    //         return Err(ResponseError::UnexpectedCommandResponse);
    //     };
    //     match data {
    //         CreateEventResponse::Event(event) => Ok(event),
    //         CreateEventResponse::Error(error) => Err(error),
    //     }
    // }
    // async fn create_governance(
    //     &self,
    //     namespace: String,
    //     payload: RequestPayload,
    //     request_signature: Signature,
    //     approvals: HashSet<ApprovalResponse>,
    //     timestamp: Option<i64>,
    // ) -> Result<Event, ResponseError> {
    //     let response = self
    //         .sender
    //         .ask(Commands::CreateEventMessage(
    //             EventRequest {
    //                 request: EventRequestType::Create(CreateRequest {
    //                     governance_id: DigestIdentifier::default(),
    //                     schema_id: String::from("governance"),
    //                     namespace,
    //                     payload,
    //                 }),
    //                 signature: request_signature,
    //                 timestamp: if timestamp.is_none() {
    //                     Utc::now().timestamp_millis()
    //                 } else {
    //                     timestamp.unwrap()
    //                 },
    //                 approvals,
    //             },
    //             true, // TODO: For now we always approve subject creation events
    //         ))
    //         .await
    //         .map_err(|_| ResponseError::ComunnicationClosed)?;
    //     let CommandManagerResponses::CreateEventResponse(data) = response else {
    //         return Err(ResponseError::UnexpectedCommandResponse);
    //     };
    //     match data {
    //         CreateEventResponse::Event(event) => Ok(event),
    //         CreateEventResponse::Error(error) => Err(error),
    //     }
    // }
}

struct CommandNotifier {
    sender: tokio::sync::broadcast::Sender<Notification>,
}

impl CommandNotifier {
    pub fn new(sender: tokio::sync::broadcast::Sender<Notification>) -> Self {
        Self { sender }
    }
}

impl NotifierInterface for CommandNotifier {
    fn subject_created(&self, id: &str) {
        let _ = self.sender.send(Notification::NewSubject {
            subject_id: id.clone().to_owned(),
            default_message: format!("Sujeto {} creado", id),
        });
    }
    fn event_created(&self, id: &str, sn: u64) {
        let _ = self.sender.send(Notification::NewEvent {
            sn,
            subject_id: id.clone().to_owned(),
            default_message: format!("Evento {} del sujeto {} creado", sn, id),
        });
    }
    fn quorum_reached(&self, id: &str, sn: u64) {
        let _ = self.sender.send(Notification::QuroumReached {
            sn,
            subject_id: id.clone().to_owned(),
            default_message: format!("Evento {} del sujeto {} ha llegado a Quorum", sn, id),
        });
    }
    fn event_signed(&self, id: &str, sn: u64) {
        let _ = self.sender.send(Notification::EventSigned {
            sn,
            subject_id: id.clone().to_owned(),
            default_message: format!("Evento {} del sujeto {} firmado", sn, id),
        });
    }
    fn subject_synchronized(&self, id: &str, sn: u64) {
        let _ = self.sender.send(Notification::SubjectSynchronized {
            subject_id: id.clone().to_owned(),
            default_message: format!("Sujeto {} sincronizado. Evento actual {}", id, sn),
        });
    }
}

pub struct CommandManager {
    inner: InnerManager<LedgerAPI, GovernanceAPI, SelfSignatureManager, CommandNotifier>,
    messenger_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
    input_channel: MpscChannel<Commands, CommandManagerResponses>,
    settings_receiver: WatchReceiver<TapleSettings>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl CommandManager {
    pub fn new(
        ledger_channel: SenderEnd<
            CommandManagerMessage,
            Result<LedgerResponse, LedgerManagerError>,
        >,
        input_channel: MpscChannel<Commands, CommandManagerResponses>,
        messenger_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
        governance_channel: SenderEnd<GovernanceMessage, GovernanceResponse>,
        keys: KeyPair,
        initial_settings: &TapleSettings,
        settings_receiver: WatchReceiver<TapleSettings>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
    ) -> Self {
        Self {
            inner: InnerManager::new(
                LedgerAPI::new(ledger_channel),
                GovernanceAPI::new(governance_channel),
                SelfSignatureManager::new(keys, &initial_settings),
                CommandNotifier::new(notification_sender),
                initial_settings.node.replication_factor,
                initial_settings.node.timeout,
            ),
            messenger_channel,
            input_channel,
            settings_receiver,
            shutdown_receiver,
            shutdown_sender,
        }
    }

    pub async fn start(&mut self) {
        let result = self.process_input().await;
        if result.is_err() {
            self.shutdown_sender
                .send(())
                .expect("Shutdown Channel Closed");
        }
    }

    pub async fn process_input(&mut self) -> Result<(), ProtocolErrors> {
        self.inner.init().await?;
        loop {
            if !self.shutdown_receiver.is_empty() {
                return Ok(());
            }
            if self.settings_receiver.has_changed().unwrap() {
                let settings = self.settings_receiver.borrow_and_update();
                self.inner.change_settings(&settings);
            }
            if let Some((signature, subject_id, sn)) = self.inner.get_pending_request() {
                let (_, tasks) = self.inner.set_signatures(signature, sn, subject_id).await?;
                for task in tasks {
                    self.messenger_channel.tell(task).await?;
                }
            }
            match self.input_channel.receive().await {
                Some(data) => match data {
                    ChannelData::AskData(data) => {
                        let (sx, content) = data.get();
                        let tasks = match content {
                            Commands::GetMessage(message) => {
                                let (event, signatures, requested_signatures) =
                                    CommandManager::extract_request_from_set(&message);
                                let mut sn: Option<u64> = None;
                                let mut tasks = Vec::new();
                                sx.send(CommandManagerResponses::GetResponse(
                                    super::CommandGetResponse {
                                        event: if event {
                                            let result = self
                                                .inner
                                                .get_event(&message.sn, &message.subject_id)
                                                .await?;
                                            sn = result.1;
                                            Some(result.0)
                                        } else {
                                            None
                                        },
                                        signatures: if signatures {
                                            let result = self
                                                .inner
                                                .get_signatures(
                                                    message.sn,
                                                    requested_signatures.unwrap(),
                                                    message.subject_id.clone(),
                                                    message.sender_id,
                                                )
                                                .await?;
                                            sn = result.1;
                                            tasks.extend(result.2);
                                            Some(result.0)
                                        } else {
                                            None
                                        },
                                        sn: sn,
                                        subject_id: message.subject_id,
                                    },
                                ))
                                .map_err(|_| ProtocolErrors::OneshotUnavailable)?;
                                tasks
                            }
                            Commands::SendMessage(message) => {
                                let mut tasks = Vec::new();
                                sx.send(CommandManagerResponses::SendResponse(
                                    CommandSendResponse {
                                        event: if message.event.is_some() {
                                            let result = self
                                                .inner
                                                .set_event(message.event.unwrap())
                                                .await?;
                                            tasks.extend(result.1);
                                            Some(result.0)
                                        } else {
                                            None
                                        },
                                        signatures: if message.signatures.is_some() {
                                            let result = self
                                                .inner
                                                .set_signatures(
                                                    message.signatures.unwrap(),
                                                    message.sn,
                                                    message.subject_id,
                                                )
                                                .await?;
                                            tasks.extend(result.1);
                                            Some(result.0)
                                        } else {
                                            None
                                        },
                                    },
                                ))
                                .map_err(|_| ProtocolErrors::OneshotUnavailable)?;
                                tasks
                            }
                            Commands::GetSingleSubject(message) => {
                                let subject = self.inner.get_subject(&message.subject_id).await?;
                                sx.send(subject)
                                    .map_err(|_| ProtocolErrors::OneshotUnavailable)?;
                                vec![]
                            }
                            Commands::GetSubjects(message) => {
                                let subjects = self.inner.get_subjects(message.namespace).await?;
                                sx.send(subjects)
                                    .map_err(|_| ProtocolErrors::OneshotUnavailable)?;
                                vec![]
                            }
                            Commands::CreateEventMessage(message, approved) => {
                                let (response, tasks) =
                                    self.inner.create_event(message, approved).await?;
                                sx.send(CommandManagerResponses::CreateEventResponse(response))
                                    .map_err(|_| ProtocolErrors::OneshotUnavailable)?;
                                tasks
                            }
                            Commands::GetSchema(message) => {
                                sx.send(
                                    self.inner
                                        .get_schema(message.governance_id, message.schema_id)
                                        .await?,
                                )
                                .map_err(|_| ProtocolErrors::OneshotUnavailable)?;
                                vec![]
                            }
                        };
                        // Send tasks to TaskManager
                        for task in tasks {
                            self.messenger_channel.tell(task).await?;
                        }
                    }
                    ChannelData::TellData(_data) => {
                        panic!("Reciving TELL in CommandManager");
                    }
                },
                None => {}
            }
        }
    }

    fn extract_request_from_set(
        content: &GetMessage,
    ) -> (bool, bool, Option<HashSet<KeyIdentifier>>) {
        let (mut event, mut signatures) = (false, false);
        let mut requested_signatures = None;
        for item in content.request_content.iter() {
            match item {
                Content::Event => event = true,
                Content::Signatures(data) => {
                    signatures = true;
                    requested_signatures = Some(data.clone())
                }
            }
        }
        (event, signatures, requested_signatures)
    }
}
