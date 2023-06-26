use crate::commons::identifier::KeyIdentifier;
use crate::commons::self_signature_manager::SelfSignatureManager;
use crate::signature::Signed;
use log::debug;
use rmp_serde;
use tokio::sync::mpsc::{self, error::SendError};

use super::{MessageContent, TaskCommandContent};

use super::{command::Command, error::Error};

const LOG_TARGET: &str = "MESSAGE_SENDER";

/// Network MessageSender struct
#[derive(Clone)]
pub struct MessageSender {
    sender: mpsc::Sender<Command>,
    controller_id: KeyIdentifier,
    signature_manager: SelfSignatureManager,
}

/// Network MessageSender implementation
impl MessageSender {
    /// New MessageSender
    pub fn new(
        sender: mpsc::Sender<Command>,
        controller_id: KeyIdentifier,
        signature_manager: SelfSignatureManager,
    ) -> Self {
        Self {
            sender,
            controller_id,
            signature_manager,
        }
    }

    /// Start listening in Taple netword
    pub async fn send_message<T: TaskCommandContent>(
        &self,
        target: KeyIdentifier,
        message: T,
    ) -> Result<(), Error> {
        // TODO: Define type of invalid identifier error
        let complete_message = Signed::<MessageContent<T>>::new(
            self.controller_id.clone(),
            target.clone(),
            message,
            &self.signature_manager,
        )?;
        let bytes = rmp_serde::to_vec(&complete_message).unwrap();
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
