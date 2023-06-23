//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::collections::HashSet;

use crate::{
    commons::errors::SubjectError,
    identifier::DigestIdentifier,
    signature::{Signature, Signed},
    EventRequestType,
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{value_wrapper::ValueWrapper, Acceptance};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EventProposal {
    pub proposal: Proposal,
    pub subject_signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    pub event_request: Signed<EventRequestType>,
    pub sn: u64,
    pub hash_prev_event: DigestIdentifier,
    pub gov_version: u64,
    pub evaluation: Option<Evaluation>,
    pub json_patch: ValueWrapper,
    pub evaluation_signatures: HashSet<Signature>,
}

impl Proposal {
    pub fn new(
        event_request: Signed<EventRequestType>,
        sn: u64,
        hash_prev_event: DigestIdentifier,
        gov_version: u64,
        evaluation: Option<Evaluation>,
        json_patch: ValueWrapper,
        evaluation_signatures: HashSet<Signature>,
    ) -> Self {
        Proposal {
            event_request,
            sn,
            hash_prev_event,
            gov_version,
            evaluation,
            json_patch,
            evaluation_signatures,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Evaluation {
    pub preevaluation_hash: DigestIdentifier,
    pub state_hash: DigestIdentifier,
    pub governance_version: u64,
    pub acceptance: Acceptance,
    pub approval_required: bool,
}

impl Signed<Proposal> {
    pub fn new(proposal: Proposal, signature: Signature) -> Self {
        Self {
            content: proposal,
            signature,
        }
    }

    pub fn verify(&self) -> Result<(), SubjectError> {
        self.signature.verify(&self.content)?;
        self.content.event_request.verify()
    }
}
