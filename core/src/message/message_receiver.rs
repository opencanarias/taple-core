use futures::StreamExt;
use rmp_serde::Deserializer;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::Cursor;
use tokio::sync::mpsc::{self};
use tokio_stream::wrappers::ReceiverStream;

use crate::{commons::channel::SenderEnd, KeyIdentifier, signature::Signed};

use super::{ TaskCommandContent, MessageContent};

#[derive(Debug)]
pub enum NetworkEvent {
    MessageReceived { message: Vec<u8> },
}

pub struct MessageReceiver<T>
where
    T: TaskCommandContent + Serialize + DeserializeOwned,
{
    receiver: ReceiverStream<NetworkEvent>,
    sender: SenderEnd<Signed<MessageContent<T>>, ()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    own_id: KeyIdentifier,
}

impl<T: TaskCommandContent + Serialize + DeserializeOwned + 'static> MessageReceiver<T> {
    pub fn new(
        receiver: mpsc::Receiver<NetworkEvent>,
        sender: SenderEnd<Signed<MessageContent<T>>, ()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        own_id: KeyIdentifier,
    ) -> Self {
        let receiver = ReceiverStream::new(receiver);
        Self {
            receiver,
            sender,
            shutdown_receiver,
            own_id,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                event = self.receiver.next() => match event {
                    Some(NetworkEvent::MessageReceived { message }) => {
                        // The message will be a string for now
                        // Deserialize the message
                        let cur = Cursor::new(message);
                        let mut de = Deserializer::new(cur);
                        let message: Signed<MessageContent<T>> = Deserialize::deserialize(&mut de).expect("Fallo de deserialización");
                        // Check message signature
                        if message.verify().is_err() || message.content.sender_id != message.signature.signer {
                            log::error!("Invalid signature in message");
                        } else if message.content.receiver != self.own_id {
                            log::error!("Message not for me");
                        } else {
                            self.sender.tell(message).await.expect("Channel Error");
                        }
                    },
                    None => {}
                },
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }
}
