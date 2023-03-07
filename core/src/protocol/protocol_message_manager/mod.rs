pub mod manager;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use crate::commons::models::approval_signature::ApprovalResponse;
use crate::commons::{
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{event::Event, signature::Signature},
};
use crate::message::TaskCommandContent;
use serde::{Deserialize, Serialize};

use super::request_manager::ApprovalRequest;

impl TaskCommandContent for ProtocolManagerMessages {}

// Definition of message structures to be used

// Messages sent to ProtocolMessageManager
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum ProtocolManagerMessages {
    GetMessage(GetMessage),
    SendMessage(SendMessage),
    ApprovalRequest(ApprovalRequest), // Sent by the network
    Vote(ApprovalResponse),           // Sent by the network
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct GetMessage {
    pub sn: EventId,
    pub subject_id: DigestIdentifier,
    pub request_content: HashSet<Content>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum EventId {
    SN { sn: u64 },
    HEAD,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum Content {
    Event,
    Signatures(HashSet<KeyIdentifier>),
}

impl Hash for Content {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Content::Event => {
                state.write_u8(0);
            }
            Content::Signatures(_) => {
                state.write_u8(1);
            }
        }
    }
}

// PUT messages (responses)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SendMessage {
    pub event: Option<Event>,
    pub sn: u64,
    pub subject_id: DigestIdentifier,
    pub signatures: Option<HashSet<Signature>>,
}
