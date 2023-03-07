use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use crate::commons::models::event_request::EventRequest;
use crate::commons::{
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{event::Event, signature::Signature, state::SubjectData},
};
use serde_json::Value;

use super::errors::ResponseError;

mod inner_manager;
pub mod manager;
pub mod self_signature_manager;
pub mod utils;

// Definition of message structures to be used

// CommandManager input messages
#[derive(Debug, Clone)]
pub enum Commands {
    GetMessage(GetMessage),
    SendMessage(SendMessage),
    CreateEventMessage(EventRequest, bool),
    GetSubjects(GetSubjects),
    GetSingleSubject(GetSingleSubject),
    GetSchema(GetSchema),
}

#[derive(Debug, Clone)]
pub struct GetSchema {
    pub governance_id: DigestIdentifier,
    pub schema_id: String,
}

#[derive(Debug, Clone)]
pub struct GetSubjects {
    pub namespace: String,
}

#[derive(Debug, Clone)]
pub struct GetSingleSubject {
    pub subject_id: DigestIdentifier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub enum EventId {
    SN { sn: u64 },
    HEAD,
}

#[derive(Debug, Clone)]
pub struct GetMessage {
    pub sn: EventId,
    pub subject_id: DigestIdentifier,
    pub sender_id: Option<KeyIdentifier>,
    pub request_content: HashSet<Content>,
}

// CommandManager output messages
#[derive(Debug, Clone)]
pub enum CommandManagerResponses {
    GetResponse(CommandGetResponse),
    SendResponse(CommandSendResponse),
    CreateEventResponse(CreateEventResponse),
    GetSubjectsResponse(Result<Vec<SubjectData>, ResponseError>),
    GetSingleSubjectResponse(Result<SubjectData, ResponseError>),
    GetSchema(Result<Value, ResponseError>),
}

#[derive(Debug, Clone)]
pub struct CommandGetResponse {
    pub event: Option<CommandGetEventResponse>,
    pub signatures: Option<CommandGetSignaturesResponse>,
    pub sn: Option<u64>,
    pub subject_id: DigestIdentifier,
}

#[derive(Debug, Clone)]
pub enum CommandGetEventResponse {
    Data(Event),
    Conflict(Conflict),
}

#[derive(Debug, Clone)]
pub enum CommandGetSignaturesResponse {
    Data(HashSet<Signature>),
    Conflict(Conflict),
}

#[derive(Debug, Clone)]
pub enum Conflict {
    SubjectNotFound,
    EventNotFound,
}

// PUT messages (responses)

#[derive(Debug, Clone)]
pub struct CommandSendResponse {
    pub event: Option<SendResponse>,
    pub signatures: Option<SendResponse>,
}

#[derive(Debug, Clone)]
pub enum CreateEventResponse {
    Event(Event),
    Error(ResponseError),
}

#[derive(Debug, Clone)]
pub enum SendResponse {
    Valid,
    Invalid,
}

#[derive(Debug, Clone)]
pub struct SendMessage {
    pub event: Option<Event>,
    pub signatures: Option<HashSet<Signature>>,
    pub sn: u64, // Before EventID
    pub subject_id: DigestIdentifier,
}
