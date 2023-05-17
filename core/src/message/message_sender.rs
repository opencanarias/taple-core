use crate::commons::identifier::{Derivable, KeyIdentifier};
use log::debug;
use rmp_serde;
use tokio::sync::mpsc::{self, error::SendError};

use super::{Message, TaskCommandContent};

use super::{command::Command, error::Error};

const LOG_TARGET: &str = "MESSAGE_SENDER";

/// Network MessageSender struct
#[derive(Clone)]
pub struct MessageSender {
    sender: mpsc::Sender<Command>,
    controller_id: KeyIdentifier,
}

/// Network MessageSender implementation
impl MessageSender {
    /// New MessageSender
    pub fn new(sender: mpsc::Sender<Command>, controller_id: KeyIdentifier) -> Self {
        Self {
            sender,
            controller_id,
        }
    }

    /// Start listening in Taple netword
    pub async fn send_message<T: TaskCommandContent>(
        &self,
        target: KeyIdentifier,
        mut message: Message<T>,
    ) -> Result<(), Error> {
        // TODO: Define type of invalid identifier error
        message.sender_id = Some(self.controller_id.clone());
        let bytes = rmp_serde::to_vec(&message).unwrap();
        log::warn!("{}: Sending message to {:?}", LOG_TARGET, target.to_str());
        debug!("{}: Sending message to {:?}", LOG_TARGET, target.to_str());
        self.sender
            .send(Command::SendMessage {
                receptor: target.public_key,
                message: bytes,
            })
            .await
            .map_err(|_| Error::ChannelClosed)?;
        Ok(())
    }

    #[allow(dead_code)]
    /// Set node as a provider of keys
    pub async fn start_providing(&mut self, keys: Vec<String>) -> Result<(), SendError<Command>> {
        self.sender.send(Command::StartProviding { keys }).await
    }

    #[allow(dead_code)]
    pub async fn bootstrap(&mut self) -> Result<(), SendError<Command>> {
        debug!("{}: Starting Bootstrap", LOG_TARGET);
        self.sender.send(Command::Bootstrap).await
    }
}
