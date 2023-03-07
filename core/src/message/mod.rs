mod command;
mod error;
mod message_receiver;
mod message_sender;
mod message_task_manager;

pub use command::*;
use crate::commons::identifier::KeyIdentifier;
pub use message_receiver::*;
pub use message_sender::*;
pub use message_task_manager::*;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Message<T: TaskCommandContent> {
    pub sender_id: Option<KeyIdentifier>,
    pub content: T,
}

pub trait TaskCommandContent: Serialize + std::fmt::Debug + Clone + Send + Sync {}

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
