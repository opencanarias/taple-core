use async_trait::async_trait;

use crate::{
    commons::{
        channel::{ChannelData, MpscChannel, SenderEnd},
        models::{event_proposal::EventProposal, Acceptance},
        self_signature_manager::SelfSignatureManager,
    },
    database::DB,
    governance::{GovernanceAPI, GovernanceUpdatedMessage},
    identifier::DigestIdentifier,
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
    utils::message::event::create_approver_response,
    DatabaseManager, Notification, TapleSettings,
};

use super::{
    error::{ApprovalErrorResponse, ApprovalManagerError},
    inner_manager::{InnerApprovalManager, RequestNotifier},
    ApprovalMessages, ApprovalPetitionData, ApprovalResponses, EmitVote,
};

pub struct ApprovalManager<D: DatabaseManager> {
    input_channel: MpscChannel<ApprovalMessages, ApprovalResponses>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    governance_update_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
    messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    inner_manager: InnerApprovalManager<GovernanceAPI, D, RequestNotifier>,
}

pub struct ApprovalAPI {
    input_channel: SenderEnd<ApprovalMessages, ApprovalResponses>,
}

impl ApprovalAPI {
    pub fn new(input_channel: SenderEnd<ApprovalMessages, ApprovalResponses>) -> Self {
        Self { input_channel }
    }
}

#[async_trait]
pub trait ApprovalAPIInterface {
    async fn request_approval(&self, data: EventProposal) -> Result<(), ApprovalErrorResponse>;
    async fn emit_vote(
        &self,
        request_id: DigestIdentifier,
        acceptance: Acceptance,
    ) -> Result<(), ApprovalErrorResponse>;
    async fn get_all_requests(&self) -> Result<Vec<ApprovalPetitionData>, ApprovalErrorResponse>;
    async fn get_single_request(
        &self,
        request_id: DigestIdentifier,
    ) -> Result<ApprovalPetitionData, ApprovalErrorResponse>;
}

#[async_trait]
impl ApprovalAPIInterface for ApprovalAPI {
    async fn request_approval(&self, data: EventProposal) -> Result<(), ApprovalErrorResponse> {
        self.input_channel
            .tell(ApprovalMessages::RequestApproval(data))
            .await
            .map_err(|_| ApprovalErrorResponse::APIChannelNotAvailable)?;
        Ok(())
    }
    async fn emit_vote(
        &self,
        request_id: DigestIdentifier,
        acceptance: Acceptance,
    ) -> Result<(), ApprovalErrorResponse> {
        let result = self
            .input_channel
            .ask(ApprovalMessages::EmitVote(EmitVote {
                request_id,
                acceptance,
            }))
            .await
            .map_err(|_| ApprovalErrorResponse::APIChannelNotAvailable)?;
        match result {
            ApprovalResponses::EmitVote(data) => data,
            _ => unreachable!(),
        }
    }
    async fn get_all_requests(&self) -> Result<Vec<ApprovalPetitionData>, ApprovalErrorResponse> {
        let result = self
            .input_channel
            .ask(ApprovalMessages::GetAllRequest)
            .await
            .map_err(|_| ApprovalErrorResponse::APIChannelNotAvailable)?;
        match result {
            ApprovalResponses::GetAllRequest(data) => Ok(data),
            _ => unreachable!(),
        }
    }
    async fn get_single_request(
        &self,
        request_id: DigestIdentifier,
    ) -> Result<ApprovalPetitionData, ApprovalErrorResponse> {
        let result = self
            .input_channel
            .ask(ApprovalMessages::GetSingleRequest(request_id))
            .await
            .map_err(|_| ApprovalErrorResponse::APIChannelNotAvailable)?;
        match result {
            ApprovalResponses::GetSingleRequest(data) => data,
            _ => unreachable!(),
        }
    }
}

impl<D: DatabaseManager> ApprovalManager<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        input_channel: MpscChannel<ApprovalMessages, ApprovalResponses>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        governance_update_channel: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
        signature_manager: SelfSignatureManager,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        settings: TapleSettings,
        database: DB<D>,
    ) -> Self {
        let passvotation = settings.node.passvotation.into();
        Self {
            input_channel,
            shutdown_sender,
            shutdown_receiver,
            messenger_channel,
            governance_update_channel,
            inner_manager: InnerApprovalManager::new(
                gov_api,
                database,
                RequestNotifier::new(notification_sender),
                signature_manager,
                passvotation,
            ),
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
                            break;
                        },
                    }
                },
                _ = self.shutdown_receiver.recv() => {
                    break;
                },
                update = self.governance_update_channel.recv() => {
                    match update {
                        Ok(data) => {
                            match data {
                                GovernanceUpdatedMessage::GovernanceUpdated { governance_id, governance_version } => {
                                    self.inner_manager.new_governance_version(&governance_id, governance_version);
                                }
                            }
                        },
                        Err(_) => {
                            self.shutdown_sender.send(()).expect("Channel Closed");
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn process_command(
        &mut self,
        command: ChannelData<ApprovalMessages, ApprovalResponses>,
    ) -> Result<(), ApprovalManagerError> {
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

        match data {
            ApprovalMessages::RequestApproval(message) => {
                if sender.is_some() {
                    return Err(ApprovalManagerError::AskNoAllowed);
                }
                let result = self.inner_manager.process_approval_request(message).await?;
                if let Ok(Some((approval, sender))) = result {
                    let msg = create_approver_response(approval);
                    self.messenger_channel
                        .tell(MessageTaskCommand::Request(
                            None,
                            msg,
                            vec![sender],
                            MessageConfig::direct_response(),
                        ))
                        .await;
                }
            }
            ApprovalMessages::EmitVote(message) => {
                match self
                    .inner_manager
                    .generate_vote(&message.request_id, message.acceptance)
                    .await?
                {
                    Ok((vote, owner)) => {
                        let msg = create_approver_response(vote);
                        self.messenger_channel
                            .tell(MessageTaskCommand::Request(
                                None,
                                msg,
                                vec![owner],
                                MessageConfig::direct_response(),
                            ))
                            .await;
                        if sender.is_some() {
                            sender.unwrap().send(ApprovalResponses::EmitVote(Ok(())));
                        }
                    }
                    Err(error) => {
                        if sender.is_some() {
                            sender
                                .unwrap()
                                .send(ApprovalResponses::EmitVote(Err(error)));
                        }
                    }
                }
            }
            ApprovalMessages::GetAllRequest => {
                let result = self.inner_manager.get_all_request();
                if sender.is_some() {
                    sender
                        .unwrap()
                        .send(ApprovalResponses::GetAllRequest(result));
                }
            }
            ApprovalMessages::GetSingleRequest(request_id) => {
                let result = self.inner_manager.get_single_request(&request_id);
                if sender.is_some() {
                    sender
                        .unwrap()
                        .send(ApprovalResponses::GetSingleRequest(result));
                }
            }
        };

        Ok(())
    }
}
