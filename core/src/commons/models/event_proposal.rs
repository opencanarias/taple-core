//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::collections::HashSet;

use crate::{
    commons::{crypto::check_cryptography, errors::SubjectError},
    event_request::EventRequest,
    identifier::DigestIdentifier,
    signature::Signature,
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::Acceptance;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EventProposal {
    pub proposal: Proposal,
    pub subject_signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    pub event_request: EventRequest,
    pub sn: u64,
    pub hash_prev_event: DigestIdentifier,
    pub gov_version: u64,
    pub evaluation: Option<Evaluation>,
    pub json_patch: String,
    pub evaluation_signatures: HashSet<Signature>,
}

impl Proposal {
    pub fn new(
        event_request: EventRequest,
        sn: u64,
        hash_prev_event: DigestIdentifier,
        gov_version: u64,
        evaluation: Option<Evaluation>,
        json_patch: String,
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

impl EventProposal {
    pub fn new(proposal: Proposal, subject_signature: Signature) -> Self {
        EventProposal {
            proposal,
            subject_signature,
        }
    }

    pub fn check_signatures(&self) -> Result<(), SubjectError> {
        check_cryptography(&self.proposal, &self.subject_signature)
            .map_err(|error| SubjectError::CryptoError(error.to_string()))?;
        log::warn!("CHECK SIGNATURES NO FALLA EN EVENT PROPOSAL");
        self.proposal.event_request.check_signatures()?;
        Ok(())
    }
}
