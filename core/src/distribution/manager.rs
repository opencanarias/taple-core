use tokio_util::sync::CancellationToken;

use crate::{
    commons::{
        channel::{ChannelData, MpscChannel, SenderEnd},
        self_signature_manager::SelfSignatureManager,
    },
    database::DB,
    governance::{GovernanceAPI, GovernanceUpdatedMessage},
    message::MessageTaskCommand,
    protocol::protocol_message_manager::TapleMessages,
    DatabaseCollection, Notification, Settings, DigestDerivator,
};

use super::{
    error::{DistributionErrorResponses, DistributionManagerError},
    inner_manager::InnerDistributionManager,
    DistributionMessagesNew,
};

pub struct DistributionManager<C: DatabaseCollection> {
    governance_update_input: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
    input_channel: MpscChannel<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
    token: CancellationToken,
    notification_tx: tokio::sync::mpsc::Sender<Notification>,
    inner_manager: InnerDistributionManager<GovernanceAPI, C>,
}

impl<C: DatabaseCollection> DistributionManager<C> {
    pub fn new(
        input_channel: MpscChannel<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
        governance_update_input: tokio::sync::broadcast::Receiver<GovernanceUpdatedMessage>,
        token: CancellationToken,
        notification_tx: tokio::sync::mpsc::Sender<Notification>,
        messenger_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        gov_api: GovernanceAPI,
        signature_manager: SelfSignatureManager,
        settings: Settings,
        db: DB<C>,
        derivator: DigestDerivator,
    ) -> Self {
        Self {
            input_channel,
            governance_update_input,
            token,
            notification_tx,
            inner_manager: InnerDistributionManager::new(
                gov_api,
                db,
                messenger_channel,
                signature_manager,
                settings,
                derivator
            ),
        }
    }

    pub async fn run(mut self) {
        if let Err(error) = self.inner_manager.init().await {
            log::error!("{}", error);
        }
        loop {
            tokio::select! {
                command = self.input_channel.receive() => {
                    match command {
                        Some(command) => {
                            let result = self.process_command(command).await;
                            if result.is_err() {
                                log::error!("{}", result.unwrap_err());
                                break;
                            }
                        }
                        None => {
                            break;
                        },
                    }
                },
                _ = self.token.cancelled() => {
                    log::debug!("Shutdown received");
                    break;
                },
                msg = self.governance_update_input.recv() => {
                    match msg {
                        Ok(data) => {
                            match data {
                                GovernanceUpdatedMessage::GovernanceUpdated{ governance_id, governance_version: _governance_version } => {
                                    if let Err(error) = self.inner_manager.governance_updated(&governance_id).await {
                                        log::error!("{}", error);
                                        break;
                                    }
                                }
                            }
                        },
                        Err(_) => {
                            break;
                        }
                    }
                }
            }
        }
        self.token.cancel();
        log::info!("Ended");
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

        let response = match data {
            DistributionMessagesNew::ProvideSignatures(data) => {
                self.inner_manager.provide_signatures(&data).await?
            }
            DistributionMessagesNew::SignaturesReceived(data) => {
                self.inner_manager.signatures_received(data).await?
            }
            DistributionMessagesNew::SignaturesNeeded { subject_id, sn } => {
                self.inner_manager
                    .start_distribution(super::StartDistribution { subject_id, sn })
                    .await?
            }
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
