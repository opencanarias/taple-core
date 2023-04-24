use serde::{Deserialize, Serialize};

use crate::{
    approval::{error::ApprovalErrorResponse, ApprovalMessages},
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    distribution::{
        error::DistributionErrorResponses, DistributionMessagesNew,
        LedgerMessages,
    },
    evaluator::{EvaluatorMessage, EvaluatorResponse},
    event::{EventCommand, EventResponse},
    message::{Message, TaskCommandContent},
    notary::{NotaryCommand, NotaryResponse},
    Event,
};


mod error;
use error::ProtocolErrors;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TapleMessages {
    DistributionMessage(DistributionMessagesNew),
    EvaluationMessage(EvaluatorMessage),
    ValidationMessage(Event),
    EventMessage(EventCommand),
    ApprovalMessages(ApprovalMessages),
    LedgerMessages(LedgerMessages),
}

impl TaskCommandContent for TapleMessages {}

pub struct ProtocolManager {
    input: MpscChannel<Message<TapleMessages>, ()>,
    distribution_sx: SenderEnd<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
    evaluation_sx: SenderEnd<EvaluatorMessage, EvaluatorResponse>,
    validation_sx: SenderEnd<NotaryCommand, NotaryResponse>,
    event_sx: SenderEnd<EventCommand, EventResponse>,
    approval_sx: SenderEnd<ApprovalMessages, Result<(), ApprovalErrorResponse>>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl ProtocolManager {
    pub async fn start(mut self) {
        loop {
            tokio::select! {
                command = self.input.receive() => {
                    match command {
                        Some(command) => {
                            let result = self.process_command(command).await;
                            if result.is_err() {
                                log::error!("Protocol Manager: {}", result.unwrap_err());
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
        &self,
        command: ChannelData<Message<TapleMessages>, ()>,
    ) -> Result<(), ProtocolErrors> {
        let msg = match command {
            ChannelData::AskData(_data) => {
                return Err(ProtocolErrors::AskCommandDetected);
            }
            ChannelData::TellData(data) => {
                let data = data.get();
                data
            }
        };
        let msg = msg.content;
        match msg {
            TapleMessages::DistributionMessage(data) => {
                self.distribution_sx
                    .tell(data)
                    .await
                    .map_err(|_| ProtocolErrors::ChannelClosed)?;
            }
            TapleMessages::EventMessage(data) => self
                .event_sx
                .tell(data)
                .await
                .map_err(|_| ProtocolErrors::ChannelClosed)?,
            TapleMessages::EvaluationMessage(data) => self
                .evaluation_sx
                .tell(data)
                .await
                .map_err(|_| ProtocolErrors::ChannelClosed)?,
            TapleMessages::ValidationMessage(data) => self
                .validation_sx
                .tell(data)
                .await
                .map_err(|_| ProtocolErrors::ChannelClosed)?,
            TapleMessages::ApprovalMessages(data) => todo!(),
            TapleMessages::LedgerMessages(_) => todo!(),
        }
        Ok(())
    }
}
