use crate::{
    commons::{
        channel::{ChannelData, MpscChannel, SenderEnd},
        self_signature_manager::SelfSignatureManager,
    },
    database::DB,
    governance::GovernanceAPI,
    message::MessageTaskCommand,
    protocol::protocol_message_manager::TapleMessages,
    DatabaseManager, TapleSettings,
};

use super::{
    error::{DistributionErrorResponses, DistributionManagerError},
    inner_manager::InnerDistributionManager,
    DistributionMessagesNew,
};

pub struct DistributionManager<D: DatabaseManager> {
    input_channel: MpscChannel<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    inner_manager: InnerDistributionManager<GovernanceAPI, D>,
}

impl<D: DatabaseManager> DistributionManager<D> {
    pub fn new(
        input_channel: MpscChannel<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        gov_api: GovernanceAPI,
        signature_manager: SelfSignatureManager,
        settings: TapleSettings,
        db: DB<D>,
    ) -> Self {
        Self {
            input_channel,
            shutdown_sender,
            shutdown_receiver,
            inner_manager: InnerDistributionManager::new(
                gov_api,
                db,
                messenger_channel,
                signature_manager,
                settings,
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
        command: ChannelData<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
    ) -> Result<(), DistributionManagerError> {
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

        log::warn!("DISTRIBUTION MSG {:?}", data);

        let response = match data {
            DistributionMessagesNew::ProvideSignatures(data) => {
                self.inner_manager.provide_signatures(&data).await?
            }
            DistributionMessagesNew::SignaturesReceived(data) => {
                self.inner_manager.signatures_received(data).await?
            }
            DistributionMessagesNew::SignaturesNeeded { subject_id, sn } => {
                self.inner_manager.start_distribution(super::StartDistribution { subject_id, sn }).await?
            },
        };
        if sender.is_some() {
            sender
                .unwrap()
                .send(response)
                .map_err(|_| DistributionManagerError::ResponseChannelNotAvailable)?;
        }
        Ok(())
    }
}
