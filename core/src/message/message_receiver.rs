use futures::StreamExt;
use rmp_serde::Deserializer;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::Cursor;
use tokio::sync::mpsc::{self};
use tokio_stream::wrappers::ReceiverStream;

use super::{Message, TaskCommandContent};

#[derive(Debug)]
pub enum NetworkEvent {
    MessageReceived { message: Vec<u8> },
}

pub struct MessageReceiver<T>
where
    T: TaskCommandContent + Serialize + DeserializeOwned,
{
    receiver: ReceiverStream<NetworkEvent>,
    sender: mpsc::Sender<Message<T>>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<T: TaskCommandContent + Serialize + DeserializeOwned + 'static> MessageReceiver<T> {
    pub fn new(
        receiver: mpsc::Receiver<NetworkEvent>,
        sender: mpsc::Sender<Message<T>>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        let receiver = ReceiverStream::new(receiver);
        Self {
            receiver,
            sender,
            shutdown_receiver,
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
                        let message: Message<T> = Deserialize::deserialize(&mut de).expect("Fallo de deserialización");
                        self.sender.send(message).await.expect("Channel Error");
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
