use commons::{
    identifier::DigestIdentifier,
    models::{
        approval_signature::{Acceptance, ApprovalResponse},
        event_request::{EventRequest, RequestData},
    },
};
use serde::{Deserialize, Serialize};

use crate::errors::ResponseError;

mod inner_manager;
pub mod manager;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum RequestManagerMessage {
    EventRequest(EventRequest), // Sent by the API. Only State type requests.
    ApprovalRequest(ApprovalRequest), // Sent by the network
    Vote(ApprovalResponse),     // Sent by the network
    VoteResolve(Acceptance, DigestIdentifier),
    GetPendingRequests,                 // Sent by the API
    GetSingleRequest(DigestIdentifier), // Sent by the API
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ApprovalRequest {
    // TODO: It is necessary to know to whom to send the answer. Signer should be enough
    // TODO: The HASH of the pre-calculation must be included
    // TODO: The controller's signature must be included
    request: EventRequest,
    expected_sn: u64,
}

#[derive(Debug, Clone)]
pub enum RequestManagerResponse {
    CreateRequest(Result<RequestData, ResponseError>), // From API
    ApprovalRequest(Result<(), ResponseError>),
    Vote(Result<(), ResponseError>),
    VoteResolve(Result<(), ResponseError>), // From API
    GetPendingRequests(Vec<EventRequest>),
    GetSingleRequest(Result<EventRequest, ResponseError>),
}

pub enum VotationType {
    Normal,
    AlwaysAccept,
    AlwaysReject,
}

impl From<u8> for VotationType {
    fn from(passvotation: u8) -> Self {
        match passvotation {
            2 => Self::AlwaysReject,
            1 => Self::AlwaysAccept,
            _ => Self::Normal,
        }
    }
}
