use serde::{Deserialize, Serialize};

use crate::{
    approval::{error::ApprovalErrorResponse, ApprovalMessages, ApprovalResponses},
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    distribution::{error::DistributionErrorResponses, DistributionMessagesNew, LedgerMessages},
    evaluator::{EvaluatorMessage, EvaluatorResponse},
    event::{EventCommand, EventResponse},
    message::{Message, TaskCommandContent},
    notary::{NotaryCommand, NotaryResponse}, ledger::{LedgerCommand, LedgerResponse},
};

mod error;
use error::ProtocolErrors;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TapleMessages {
    DistributionMessage(DistributionMessagesNew),
    EvaluationMessage(EvaluatorMessage),
    ValidationMessage(NotaryCommand),
    EventMessage(EventCommand),
    ApprovalMessages(ApprovalMessages),
    LedgerMessages(LedgerCommand),
}

impl TaskCommandContent for TapleMessages {}

pub struct ProtocolManager {
    input: MpscChannel<Message<TapleMessages>, ()>,
    distribution_sx: SenderEnd<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
    #[cfg(feature = "evaluation")]
    evaluation_sx: SenderEnd<EvaluatorMessage, EvaluatorResponse>,
    #[cfg(feature = "validation")]
    validation_sx: SenderEnd<NotaryCommand, NotaryResponse>,
    event_sx: SenderEnd<EventCommand, EventResponse>,
    #[cfg(feature = "aproval")]
    approval_sx: SenderEnd<ApprovalMessages, ApprovalResponses>,
    ledger_sx: SenderEnd<LedgerCommand, LedgerResponse>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl ProtocolManager {
    pub fn new(
        input: MpscChannel<Message<TapleMessages>, ()>,
        distribution_sx: SenderEnd<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
        #[cfg(feature = "evaluation")]
        evaluation_sx: SenderEnd<EvaluatorMessage, EvaluatorResponse>,
        #[cfg(feature = "validation")]
        validation_sx: SenderEnd<NotaryCommand, NotaryResponse>,
        event_sx: SenderEnd<EventCommand, EventResponse>,
        #[cfg(feature = "aproval")]
        approval_sx: SenderEnd<ApprovalMessages, ApprovalResponses>,
        ledger_sx: SenderEnd<LedgerCommand, LedgerResponse>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
    ) -> Self {
        Self {
            input,
            distribution_sx,
            #[cfg(feature = "evaluation")]
            evaluation_sx,
            #[cfg(feature = "validation")]
            validation_sx,
            event_sx,
            #[cfg(feature = "aproval")]
            approval_sx,
            ledger_sx,
            shutdown_receiver: shutdown_sender.subscribe(),
            shutdown_sender,
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
        //println!("MSG PROTOCOL {:?}", msg);
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
            TapleMessages::EvaluationMessage(data) => {
                log::warn!("Evaluation Message Received");
                #[cfg(feature = "evaluation")]
                {
                    return Ok(self
                        .evaluation_sx
                        .tell(data)
                        .await
                        .map_err(|_| ProtocolErrors::ChannelClosed)?);
                }
                #[cfg(not(feature = "evaluation"))]
                log::trace!("Evaluation Message received. Current node is not able to evaluate");
            }
            TapleMessages::ValidationMessage(data) => {
                #[cfg(feature = "validation")]
                {
                    return Ok(self
                    .validation_sx
                    .tell(data)
                    .await
                    .map_err(|_| ProtocolErrors::ChannelClosed)?);
                }
                #[cfg(not(feature = "validation"))]
                log::trace!("Validation Message received. Current node is not able to validate");
            }
            TapleMessages::ApprovalMessages(data) => {
                #[cfg(feature = "aproval")]
                {
                    return Ok(self
                        .approval_sx
                        .tell(data)
                        .await
                        .map_err(|_| ProtocolErrors::ChannelClosed)?);
                }
                #[cfg(not(feature = "aproval"))]
                log::trace!("Aproval Message received. Current node is not able to aprove");
            }
            TapleMessages::LedgerMessages(data) => self
            .ledger_sx
            .tell(data)
            .await
            .map_err(|_| ProtocolErrors::ChannelClosed)?,
        }
        Ok(())
    }
}