pub mod manager;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use commons::models::approval_signature::ApprovalResponse;
use commons::{
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{event::Event, signature::Signature},
};
use message::TaskCommandContent;
use serde::{Deserialize, Serialize};

use crate::request_manager::ApprovalRequest;

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

/*#[derive(Debug)]
pub struct ProtocolMessageGetResponse {
    event: Option<ProtocolMessageGetEventResponse>,
    signatures: Option<ProtocolMessagesGetSignaturesResponse>,
}*/

/*#[derive(Debug)]
enum ProtocolMessageGetEventResponse {
    Data(Event),
    Conflict(Conflict),
}*/

#[derive(Debug)]
pub enum ProtocolMessagesGetSignaturesResponse {
    Data(HashSet<Signature>),
    Conflict(Conflict),
}

#[derive(Debug)]
pub enum Conflict {
    SubjectNotFound,
    EventNotFound,
}

// PUT messages (responses)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SendMessage {
    pub event: Option<Event>,
    pub sn: u64,
    pub subject_id: DigestIdentifier,
    pub signatures: Option<HashSet<Signature>>,
}

#[derive(Debug)]
pub enum SendResponse {
    Valid,
    Invalid,
}
