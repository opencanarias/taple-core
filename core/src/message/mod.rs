mod command;
mod error;
mod message_receiver;
mod message_sender;
mod message_task_manager;

use crate::{
    commons::{
        errors::SubjectError,
        identifier::KeyIdentifier,
        models::HashId,
        self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
    },
    signature::{Signature, Signed},
    DigestIdentifier,
};
use borsh::{BorshDeserialize, BorshSerialize};
pub use command::*;
pub use message_receiver::*;
pub use message_sender::*;
pub use message_task_manager::*;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use self::error::Error;

// #[derive(Debug, Clone, Deserialize, Serialize, PartialEq, BorshSerialize, BorshDeserialize)]
// pub struct Message<T: TaskCommandContent> {
//     pub content: MessageContent<T>,
//     pub signature: Signature,
// }

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct MessageContent<T: TaskCommandContent> {
    pub sender_id: KeyIdentifier,
    pub content: T,
    pub receiver: KeyIdentifier,
}

impl<T: TaskCommandContent> HashId for MessageContent<T> {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self)
            .map_err(|_| SubjectError::CryptoError("Hashing error in MessageContent".to_string()))
    }
}

impl<T: TaskCommandContent> Signed<MessageContent<T>> {
    pub fn new(
        sender: KeyIdentifier,
        receiver: KeyIdentifier,
        content: T,
        sender_sm: &SelfSignatureManager,
    ) -> Result<Self, Error> {
        let message_content = MessageContent {
            sender_id: sender.clone(),
            content,
            receiver,
        };
        let signature = sender_sm
            .sign(&message_content)
            .map_err(|_| Error::CreatingMessageError)?;
        Ok(Self {
            content: message_content,
            signature,
        })
    }

    pub fn verify(&self) -> Result<(), SubjectError> {
        self.signature.verify(&self.content)
    }
}

// impl<T: TaskCommandContent> Message<T> {
//     pub fn new(
//         sender: KeyIdentifier,
//         receiver: KeyIdentifier,
//         content: T,
//         sender_sm: &SelfSignatureManager,
//     ) -> Result<Self, Error> {
//         let content = MessageContent {
//             sender_id: sender.clone(),
//             content,
//             receiver,
//         };
//         let content_hash = DigestIdentifier::from_serializable_borsh(&content)
//             .map_err(|_| Error::CreatingMessageError)?;
//         let signature = sender_sm
//             .sign(&content_hash)
//             .map_err(|_| Error::CreatingMessageError)?;
//         Ok(Self { content, signature })
//     }

//     pub fn verify(&self) -> Result<(), SubjectError> {
//         self.signature.verify(&self.content)
//     }
// }

pub trait TaskCommandContent:
    Serialize + std::fmt::Debug + Clone + Send + Sync + BorshDeserialize + BorshSerialize
{
}

#[derive(Clone, Debug, PartialEq)]
pub enum MessageTaskCommand<M>
where
    M: TaskCommandContent + Serialize + DeserializeOwned,
{
    Request(Option<String>, M, Vec<KeyIdentifier>, MessageConfig), // Order (once or indefinitely)
    Cancel(String), // Exists to cancel the previous one
}

#[derive(Clone, Debug, PartialEq)]
pub struct MessageConfig {
    pub timeout: u32,
    pub replication_factor: f64,
}

impl MessageConfig {
    pub fn timeout(&self) -> u32 {
        self.timeout
    }

    pub fn replication_factor(&self) -> f64 {
        self.replication_factor
    }

    pub fn direct_response() -> Self {
        Self {
            timeout: 0,
            replication_factor: 1.0,
        }
    }
}
