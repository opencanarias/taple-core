use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    governance::GovernanceAPI,
    message::MessageTaskCommand,
};

use super::{
    error::{ApprovalErrorResponse, ApprovalManagerError},
    ApprovalMessages,
};

struct ApprovalManager {
    gov_api: GovernanceAPI,
    input_channel: MpscChannel<ApprovalMessages, Result<(), ApprovalErrorResponse>>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    messenger_channel: SenderEnd<MessageTaskCommand<ApprovalMessages>, ()>,
}

impl ApprovalManager {
    pub fn new(
        gov_api: GovernanceAPI,
        input_channel: MpscChannel<ApprovalMessages, Result<(), ApprovalErrorResponse>>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        messenger_channel: SenderEnd<MessageTaskCommand<ApprovalMessages>, ()>,
    ) -> Self {
        Self {
            gov_api,
            input_channel,
            shutdown_sender,
            shutdown_receiver,
            messenger_channel,
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
        let data = match command {
            ChannelData::AskData(_) => {
                return Err(ApprovalManagerError::AskNoAllowed);
            }
            ChannelData::TellData(data) => {
                let data = data.get();
                data
            }
        };

        todo!();
    }
}
