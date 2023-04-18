use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    database::DB,
    governance::GovernanceAPI,
    message::{MessageConfig, MessageTaskCommand},
    protocol::{
        command_head_manager::self_signature_manager::SelfSignatureManager,
    },
    DatabaseManager, Notification, TapleSettings,
};

use super::{
    error::{ApprovalErrorResponse, ApprovalManagerError},
    inner_manager::{InnerApprovalManager, RequestNotifier},
    ApprovalMessages, VoteMessage,
};

struct ApprovalManager<D: DatabaseManager> {
    input_channel: MpscChannel<ApprovalMessages, Result<(), ApprovalErrorResponse>>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    messenger_channel: SenderEnd<MessageTaskCommand<VoteMessage>, ()>,
    inner_manager: InnerApprovalManager<GovernanceAPI, D, RequestNotifier>,
}

impl<D: DatabaseManager> ApprovalManager<D> {
    pub fn new(
        gov_api: GovernanceAPI,
        input_channel: MpscChannel<ApprovalMessages, Result<(), ApprovalErrorResponse>>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        messenger_channel: SenderEnd<MessageTaskCommand<VoteMessage>, ()>,
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
        command: ChannelData<ApprovalMessages, Result<(), ApprovalErrorResponse>>,
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
                if let Ok(Some((msg_to_send, sender))) = result {
                    self.messenger_channel
                        .tell(MessageTaskCommand::Request(
                            None,
                            msg_to_send,
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
                        self.messenger_channel
                            .tell(MessageTaskCommand::Request(
                                None,
                                vote,
                                vec![owner],
                                MessageConfig::direct_response(),
                            ))
                            .await;
                        if sender.is_some() {
                            sender.unwrap().send(Ok(()));
                        }
                    }
                    Err(error) => {
                        if sender.is_some() {
                            sender.unwrap().send(Err(error));
                        }
                    }
                }
            }
        };

        Ok(())
    }
}
