//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use borsh::{BorshDeserialize, BorshSerialize};
use utoipa::ToSchema;
use serde::{Deserialize, Serialize};
use crate::{event_request::EventRequest, identifier::{DigestIdentifier, KeyIdentifier}, signature::Signature};

use super::Acceptance;

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct EventProposal {
    pub event_request: EventRequest,
    pub sn: u64,
    pub evaluation: Evaluation,
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
    pub json_patch: String,
}