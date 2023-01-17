use std::collections::HashSet;

use super::{
    Content as ContentProtocol, EventId as EventIdProtocol, GetMessage as GetMessageProtocol,
    ProtocolManagerMessages, SendMessage as SendMessageProtocol,
};
use commons::{
    channel::SenderEnd,
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{event::Event, signature::Signature},
};

use crate::{
    command_head_manager::{
        CommandGetEventResponse, CommandGetSignaturesResponse, CommandManagerResponses, Commands,
        Content, EventId, GetMessage, SendMessage,
    },
    request_manager::{RequestManagerMessage, RequestManagerResponse},
};

use message::{Message, MessageConfig, MessageTaskCommand};

impl Into<EventId> for EventIdProtocol {
    fn into(self) -> EventId {
        match self {
            Self::HEAD => EventId::HEAD,
            Self::SN { sn } => EventId::SN { sn: sn },
        }
    }
}

impl Into<GetMessage> for (KeyIdentifier, GetMessageProtocol) {
    fn into(self) -> GetMessage {
        let mut request_content = HashSet::new();
        for content in self.1.request_content {
            request_content.insert(match content {
                ContentProtocol::Event => Content::Event,
                ContentProtocol::Signatures(data) => Content::Signatures(data),
            });
        }
        GetMessage {
            sn: self.1.sn.into(),
            subject_id: self.1.subject_id,
            sender_id: Some(self.0),
            request_content: request_content,
        }
    }
}

pub struct ProtocolMessageManager {
    request_manager_channel: SenderEnd<RequestManagerMessage, RequestManagerResponse>,
    commands_manager_channel: SenderEnd<Commands, CommandManagerResponses>,
    messages_reciver_channel: tokio::sync::mpsc::Receiver<Message<ProtocolManagerMessages>>,
    messenger_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
    _shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl ProtocolMessageManager {
    pub fn new(
        request_manager_channel: SenderEnd<RequestManagerMessage, RequestManagerResponse>,
        commands_manager_channel: SenderEnd<Commands, CommandManagerResponses>,
        messages_reciver_channel: tokio::sync::mpsc::Receiver<Message<ProtocolManagerMessages>>,
        messenger_channel: SenderEnd<MessageTaskCommand<ProtocolManagerMessages>, ()>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        Self {
            request_manager_channel,
            commands_manager_channel,
            messages_reciver_channel,
            messenger_channel,
            _shutdown_sender: shutdown_sender,
            shutdown_receiver,
        }
    }

    pub async fn start(&mut self) {
        loop {
            tokio::select! {
                msg = self.messages_reciver_channel.recv() => {
                    self.process_input(msg).await; // TODO: Define error handling
                },
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }

    async fn process_input(&mut self, input: Option<Message<ProtocolManagerMessages>>) {
        match input {
            Some(data) => {
                let sender_id = data.sender_id.unwrap(); // TODO: Must always have a sender
                match data.content {
                    ProtocolManagerMessages::GetMessage(message) => {
                        if message.request_content.len() == 0 {
                            return;
                        }
                        let message: GetMessage = (sender_id.clone(), message).into();
                        let subject_id = message.subject_id.clone();
                        let response = self
                            .commands_manager_channel
                            .ask(Commands::GetMessage(message))
                            .await
                            .unwrap();
                        match response {
                            CommandManagerResponses::GetResponse(data) => {
                                // TODO: Analyze if nothing should be sent if the ledger lacks the event
                                let event = match data.event {
                                    Some(event) => match event {
                                        CommandGetEventResponse::Data(data) => Some(data),
                                        CommandGetEventResponse::Conflict(_) => {
                                            return;
                                        }
                                    },
                                    None => None,
                                };
                                let signature = match data.signatures {
                                    Some(CommandGetSignaturesResponse::Data(signatures)) => {
                                        if signatures.len() == 0 {
                                            None
                                        } else {
                                            Some(signatures)
                                        }
                                    }
                                    Some(CommandGetSignaturesResponse::Conflict(_)) => return,
                                    _ => None,
                                };
                                if event.is_none() && signature.is_none() {
                                    return;
                                }
                                self.messenger_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        ProtocolMessageManager::build_send_data(
                                            event,
                                            signature,
                                            data.sn.unwrap(),
                                            subject_id,
                                        ),
                                        vec![sender_id],
                                        MessageConfig::direct_response(),
                                    ))
                                    .await
                                    .expect("Channel Protocol-Task closed");
                            }
                            _ => unreachable!(),
                        }
                    }
                    ProtocolManagerMessages::SendMessage(message) => {
                        let to_send = Commands::SendMessage(SendMessage {
                            event: message.event,
                            signatures: message.signatures,
                            sn: message.sn,
                            subject_id: message.subject_id,
                        });
                        let response = self.commands_manager_channel.ask(to_send).await.unwrap();
                        match response {
                            _ => {}
                        }
                    }
                    ProtocolManagerMessages::ApprovalRequest(approval_request) => self
                        .request_manager_channel
                        .tell(RequestManagerMessage::ApprovalRequest(approval_request))
                        .await
                        .unwrap(),
                    ProtocolManagerMessages::Vote(approval) => self
                        .request_manager_channel
                        .tell(RequestManagerMessage::Vote(approval))
                        .await
                        .unwrap(),
                }
            }
            None => {}
        }
    }

    pub fn build_send_data(
        event: Option<Event>,
        signatures: Option<HashSet<Signature>>,
        sn: u64,
        subject_id: DigestIdentifier,
    ) -> ProtocolManagerMessages {
        ProtocolManagerMessages::SendMessage(SendMessageProtocol {
            event,
            signatures,
            sn,
            subject_id,
        })
    }
}

#[cfg(test)]
mod test {}
