//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::collections::HashSet;

use crate::{event_request::EventRequest, identifier::DigestIdentifier, signature::Signature};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::Acceptance;

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct EventProposal {
    pub proposal: Proposal,
    pub subject_signature: Signature,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct Proposal {
    pub event_request: EventRequest,
    pub sn: u64,
    pub gov_version: u64,
    pub evaluation: Option<Evaluation>,
    pub json_patch: String,
    pub evaluation_signatures: HashSet<Signature>,
}

impl Proposal {
    pub fn new(
        event_request: EventRequest,
        sn: u64,
        gov_version: u64,
        evaluation: Option<Evaluation>,
        json_patch: String,
        evaluation_signatures: HashSet<Signature>,
    ) -> Self {
        Proposal {
            event_request,
            sn,
            gov_version,
            evaluation,
            json_patch,
            evaluation_signatures,
        }
    }
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
    pub fn new(proposal: Proposal, subject_signature: Signature) -> Self {
        EventProposal {
            proposal,
            subject_signature,
        }
    }
}
