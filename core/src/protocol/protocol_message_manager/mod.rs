use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    approval::{ApprovalMessages, ApprovalResponses},
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    distribution::{error::DistributionErrorResponses, DistributionMessagesNew},
    evaluator::{EvaluatorMessage, EvaluatorResponse},
    event::{EventCommand, EventResponse},
    ledger::{LedgerCommand, LedgerResponse},
    message::{MessageContent, TaskCommandContent},
    notary::{NotaryCommand, NotaryResponse},
    signature::Signed,
};

mod error;
use error::ProtocolErrors;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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
    input: MpscChannel<Signed<MessageContent<TapleMessages>>, ()>,
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
        input: MpscChannel<Signed<MessageContent<TapleMessages>>, ()>,
        distribution_sx: SenderEnd<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
        #[cfg(feature = "evaluation")] evaluation_sx: SenderEnd<
            EvaluatorMessage,
            EvaluatorResponse,
        >,
        #[cfg(feature = "validation")] validation_sx: SenderEnd<NotaryCommand, NotaryResponse>,
        event_sx: SenderEnd<EventCommand, EventResponse>,
        #[cfg(feature = "aproval")] approval_sx: SenderEnd<ApprovalMessages, ApprovalResponses>,
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
        command: ChannelData<Signed<MessageContent<TapleMessages>>, ()>,
    ) -> Result<(), ProtocolErrors> {
        let message = match command {
            ChannelData::AskData(_data) => {
                return Err(ProtocolErrors::AskCommandDetected);
            }
            ChannelData::TellData(data) => {
                let data = data.get();
                data
            }
        };
        let msg = message.content.content;
        let sender = message.content.sender_id;
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
                #[cfg(feature = "evaluation")]
                {
                    let evaluation_command = match data {
                        EvaluatorMessage::EvaluationEvent {
                            ..
                        } => {
                            log::error!("Evaluation Event Received in protocol manager");
                            return Ok(());
                        }
                        EvaluatorMessage::AskForEvaluation(evaluation_request) => {
                            EvaluatorMessage::EvaluationEvent {
                                evaluation_request,
                                sender,
                            }
                        }
                    };
                    return Ok(self
                        .evaluation_sx
                        .tell(evaluation_command)
                        .await
                        .map_err(|_| ProtocolErrors::ChannelClosed)?);
                }
                #[cfg(not(feature = "evaluation"))]
                log::trace!("Evaluation Message received. Current node is not able to evaluate");
            }
            TapleMessages::ValidationMessage(data) => {
                #[cfg(feature = "validation")]
                {
                    let notary_command = match data {
                        NotaryCommand::NotaryEvent {
                            ..
                        } => {
                            log::error!("Notary Event Received in protocol manager");
                            return Ok(());
                        }
                        NotaryCommand::AskForNotary(notary_event) => NotaryCommand::NotaryEvent {
                            notary_event,
                            sender,
                        },
                    };
                    return Ok(self
                        .validation_sx
                        .tell(notary_command)
                        .await
                        .map_err(|_| ProtocolErrors::ChannelClosed)?);
                }
                #[cfg(not(feature = "validation"))]
                log::trace!("Validation Message received. Current node is not able to validate");
            }
            TapleMessages::ApprovalMessages(data) => {
                #[cfg(feature = "aproval")]
                {
                    let approval_command = match data {
                        ApprovalMessages::RequestApproval(approval) => {
                            ApprovalMessages::RequestApprovalWithSender { approval, sender }
                        }
                        ApprovalMessages::RequestApprovalWithSender { .. } => {
                            log::error!(
                                "Request Approval with Sender Received in protocol manager"
                            );
                            return Ok(());
                        }
                        _ => data,
                    };
                    return Ok(self
                        .approval_sx
                        .tell(approval_command)
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
