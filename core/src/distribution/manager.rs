use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    database::DB,
    governance::GovernanceAPI,
    message::MessageTaskCommand,
    protocol::command_head_manager::self_signature_manager::SelfSignatureManager,
    DatabaseManager, Notification,
};

use super::{
    error::{DistributionErrorResponses, DistributionManagerError},
    inner_manager::{DistributionNotifier, InnerDistributionManager},
    DistributionMessages,
};

// En principio los mensajes no los enviará este módulo sino que habrá otro encargado.
// El módulo solo recibirá mensajes provenientes de la red. Ninguno de la API.
pub struct DistributionManager<D: DatabaseManager> {
    input: MpscChannel<DistributionMessages, Result<(), DistributionErrorResponses>>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    inner: InnerDistributionManager<D, GovernanceAPI, DistributionNotifier>,
}

impl<D: DatabaseManager> DistributionManager<D> {
    pub fn new(
        governance: GovernanceAPI,
        input: MpscChannel<DistributionMessages, Result<(), DistributionErrorResponses>>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        db: DB<D>,
        signature_manager: SelfSignatureManager,
        notification_sender: tokio::sync::broadcast::Sender<Notification>,
        messenger_channel: SenderEnd<MessageTaskCommand<DistributionMessages>, ()>,
    ) -> Self {
        Self {
            input,
            shutdown_sender,
            shutdown_receiver,
            inner: InnerDistributionManager::new(
                db,
                governance,
                signature_manager,
                DistributionNotifier::new(notification_sender),
                messenger_channel,
            ),
        }
    }

    pub async fn start(mut self) {
        loop {
            tokio::select! {
                command = self.input.receive() => {
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
                        },
                    }
                },
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }

    pub async fn process_command(
        &mut self,
        command: ChannelData<DistributionMessages, Result<(), DistributionErrorResponses>>,
    ) -> Result<(), DistributionManagerError> {
        let (sender, data) = match command {
            ChannelData::AskData(data) => {
                let (sender, data) = data.get();
                (Some(sender), data)
            }
            ChannelData::TellData(_) => {
                return Err(DistributionManagerError::TellNoAllowed);
            }
        };

        let response = match data {
            DistributionMessages::SetEvent(message) => self.inner.set_event(message).await?,
            DistributionMessages::RequestSignature(message) => {
                self.inner.request_signatures(message).await?
            }
            DistributionMessages::SignaturesReceived(message) => {
                self.inner.signature_received(message).await?
            }
            DistributionMessages::RequestEvent(message) => {
                self.inner.request_event(message).await?
            }
        };

        sender
            .unwrap()
            .send(response)
            .map_err(|_| DistributionManagerError::ResponseChannelNotAvailable)?;
        Ok(())
    }
}
