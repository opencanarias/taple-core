//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use crate::{
    event_request::EventRequest,
    identifier::{DigestIdentifier, KeyIdentifier},
    signature::Signature,
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::Acceptance;

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct EventProposal {
    pub event_request: EventRequest,
    pub sn: u64,
    pub evaluation: Evaluation,
    pub json_patch: String,
    pub evaluation_signatures: Vec<Signature>,
    pub subject_signature: Signature,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct Evaluation {
    #[schema(value_type = String)]
    pub preevaluation_hash: DigestIdentifier,
    #[schema(value_type = String)]
    pub state_hash: DigestIdentifier,
    pub governance_version: u64,
    pub acceptance: Acceptance,
    pub approval_required: bool,
}

impl EventProposal {
    pub fn new(
        event_request: EventRequest,
        sn: u64,
        evaluation: Evaluation,
        evaluation_signatures: Vec<Signature>,
        subject_signature: Signature,
        json_patch: String,
    ) -> Self {
        EventProposal {
            event_request,
            sn,
            evaluation,
            evaluation_signatures,
            subject_signature,
            json_patch,
        }
    }
}
