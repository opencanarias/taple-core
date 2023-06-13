//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::hash::Hasher;

use super::Acceptance;
use crate::{identifier::DigestIdentifier, signature::Signature};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    PartialOrd,
    PartialEq,
    Hash,
)]
pub struct Approval {
    pub content: ApprovalContent,
    pub signature: Signature,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    PartialOrd,
    PartialEq,
    Hash,
)]
pub struct ApprovalContent {
    pub event_proposal_hash: DigestIdentifier,
    pub acceptance: Acceptance,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, BorshSerialize, BorshDeserialize, PartialOrd,
)]
pub struct UniqueApproval {
    pub approval: Approval,
}

impl PartialEq for UniqueApproval {
    fn eq(&self, other: &Self) -> bool {
        self.approval.signature.content.signer == other.approval.signature.content.signer
    }
}

impl Hash for UniqueApproval {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.approval.signature.content.signer.hash(state);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema)]
pub enum ApprovalStatus {
    Pending,
    Voted
}