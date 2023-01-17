use async_trait::async_trait;
use commons::{
    bd::db::DB,
    channel::{ChannelData, MpscChannel, SenderEnd},
    config::TapleSettings,
    crypto::KeyPair,
    identifier::DigestIdentifier,
    models::{
        approval_signature::{Acceptance, ApprovalResponse},
        event_request::{EventRequest, RequestData},
        notification::Notification,
    },
};
use governance::{GovernanceAPI, GovernanceMessage, GovernanceResponse};
use message::MessageTaskCommand;

use crate::{
    command_head_manager::{
        manager::CommandAPI, self_signature_manager::SelfSignatureManager, CommandManagerResponses,
        Commands,
    },
    errors::{RequestManagerError, ResponseError},
    protocol_message_manager::ProtocolManagerMessages,
};

use super::{
    inner_manager::{InnerManager, RequestNotifier},
    ApprovalRequest, RequestManagerMessage, RequestManagerResponse,
};

#[async_trait]
pub trait RequestManagerInterface {
    async fn event_request(
        &self,
        event_request: EventRequest,
    ) -> Result<RequestData, ResponseError>;
    async fn approval_request(
        &self,
        approval_request: ApprovalRequest,
    ) -> Result<(), ResponseError>;
    async fn approval(&self, approval: ApprovalResponse) -> Result<(), ResponseError>;
    async fn approval_resolve(
        &self,
        acceptance: Acceptance,
        request_id: DigestIdentifier,
    ) -> Result<(), ResponseError>;
    async fn get_pending_requests(&self) -> Result<Vec<EventRequest>, ResponseError>;
    async fn get_single_pending_request(
        &self,
        id: DigestIdentifier,
    ) -> Result<EventRequest, ResponseError>;
}

pub struct RequestManagerAPI {
    sender: SenderEnd<RequestManagerMessage, RequestManagerResponse>,
}

impl RequestManagerAPI {
    pub fn new(sender: SenderEnd<RequestManagerMessage, RequestManagerResponse>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl RequestManagerInterface for RequestManagerAPI {
    async fn event_request(
        &self,
        event_request: EventRequest,
    ) -> Result<RequestData, ResponseError> {
        let result = self
            .sender
            .ask(RequestManagerMessage::EventRequest(event_request))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        if let RequestManagerResponse::CreateRequest(data) = result {
            data
        } else {
            unreachable!()
        }
    }
    async fn approval_request(
        &self,
        approval_request: ApprovalRequest,
    ) -> Result<(), ResponseError> {
        Ok(self
            .sender
            .tell(RequestManagerMessage::ApprovalRequest(approval_request))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?)
    }
    async fn approval(&self, approval: ApprovalResponse) -> Result<(), ResponseError> {
        Ok(self
            .sender
            .tell(RequestManagerMessage::Vote(approval))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?)
    }
    async fn approval_resolve(
        &self,
        acceptance: Acceptance,
        request_id: DigestIdentifier,
    ) -> Result<(), ResponseError> {
        let data = self
            .sender
            .ask(RequestManagerMessage::VoteResolve(acceptance, request_id))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        match data {
            RequestManagerResponse::VoteResolve(res) => res,
            _ => unreachable!(""),
        }
    }

    async fn get_pending_requests(&self) -> Result<Vec<EventRequest>, ResponseError> {
        let data = self
            .sender
            .ask(RequestManagerMessage::GetPendingRequests)
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        match data {
            RequestManagerResponse::GetPendingRequests(res) => Ok(res),
            _ => unreachable!(""),
        }
    }

    async fn get_single_pending_request(
        &self,
        id: DigestIdentifier,
    ) -> Result<EventRequest, ResponseError> {
        let data = self
            .sender
            .ask(RequestManagerMessage::GetSingleRequest(id))
            .await
            .map_err(|_| ResponseError::ComunnicationClosed)?;
        match data {
            RequestManagerResponse::GetSingleRequest(res) => res,
            _ => unreachable!(""),
        }
    }
}

pub struct RequestManager {
    input: MpscChannel<RequestManagerMessage, RequestManagerResponse>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    messenger_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
    inner_manager:
        InnerManager<DB, RequestNotifier, CommandAPI, GovernanceAPI, SelfSignatureManager>,
}

impl RequestManager {
    pub fn new(
        // TODO: Requires access to the database and TaskManager. Ask if it requires the settings channel.
        // TODO: selfSignatureManager may be required.
        input: MpscChannel<RequestManagerMessage, RequestManagerResponse>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        messenger_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
        command_channel: SenderEnd<Commands, CommandManagerResponses>,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        governance_channel: SenderEnd<GovernanceMessage, GovernanceResponse>,
        db: DB,
        keys: KeyPair,
        initial_settings: &TapleSettings,
    ) -> Self {
        Self {
            input,
            shutdown_sender,
            shutdown_receiver,
            messenger_channel,
            inner_manager: InnerManager::new(
                db,
                RequestNotifier::new(notification_sender),
                CommandAPI::new(command_channel),
                governance::GovernanceAPI::new(governance_channel),
                SelfSignatureManager::new(keys, initial_settings),
                initial_settings.node.passvotation.into(),
            ),
        }
    }

    pub async fn start(&mut self) {
        if let Err(error) = self.inner_manager.init().await {
            log::error!("Request Manager fail: {:?}", error);
            self.shutdown_sender.send(()).unwrap();
            return;
        };
        loop {
            tokio::select! {
                biased;
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
                msg = self.input.receive() => {
                    if let Err(error) = self.process_input(msg).await {
                        log::error!("Request Manager fail: {:?}", error);
                        self.shutdown_sender.send(()).unwrap();
                        break;
                    };
                },
            }
        }
    }

    pub async fn process_input(
        &mut self,
        msg: Option<ChannelData<RequestManagerMessage, RequestManagerResponse>>,
    ) -> Result<(), RequestManagerError> {
        // Ask for request
        // Tell for the rest
        let Some(msg) = msg else {
            return Err(RequestManagerError::ChannelClosed);
        };
        let (sender, data) = match msg {
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
            let (result, task) = match data {
                // TODO: TaskManager message management
                RequestManagerMessage::Vote(approval) => {
                    let (result, task) = self.inner_manager.process_approval(approval).await?;
                    // Process vote
                    (result, task)
                }
                RequestManagerMessage::ApprovalRequest(approval_request) => {
                    let (result, task) = self
                        .inner_manager
                        .process_approval_request(approval_request)
                        .await?;
                    // Process ApprovalRequest
                    (result, task)
                }
                RequestManagerMessage::EventRequest(request) => {
                    let (result, task) = self.inner_manager.process_request(request).await?;
                    (result, task)
                }
                RequestManagerMessage::VoteResolve(acceptance, request_id) => {
                    let (result, task) = self
                        .inner_manager
                        .process_approval_resolve(&request_id, acceptance)
                        .await?;
                    (result, task)
                }
                RequestManagerMessage::GetPendingRequests => {
                    let result = self.inner_manager.get_pending_request();
                    (result, None)
                }
                RequestManagerMessage::GetSingleRequest(id) => {
                    let result = self.inner_manager.get_single_request(id);
                    (result, None)
                }
            };
            if let Some(message) = task {
                self.messenger_channel
                    .tell(message)
                    .await
                    .map_err(|_| RequestManagerError::ChannelClosed)?
            }
            result
        };
        if sender.is_some() {
            sender
                .unwrap()
                .send(response)
                .expect("Se ha enviado la respuesta");
        }
        Ok(())
    }
}
