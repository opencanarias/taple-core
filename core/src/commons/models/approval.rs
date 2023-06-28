//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::hash::Hasher;

use crate::{
    commons::errors::SubjectError,
    identifier::DigestIdentifier,
    signature::{Signature, Signed},
    EventRequest, ValueWrapper,
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

use super::HashId;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct ApprovalRequest {
    // Evaluation Request
    pub event_request: Signed<EventRequest>,
    pub sn: u64,
    pub gov_version: u64,
    // Evaluation Response
    pub patch: ValueWrapper, // cambiar
    pub state_hash: DigestIdentifier,
    pub hash_prev_event: DigestIdentifier,
}

impl HashId for ApprovalRequest {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self).map_err(|_| {
            SubjectError::SignatureCreationFails("HashId for ApprovalRequest Fails".to_string())
        })
    }
}

impl Signed<ApprovalRequest> {
    pub fn varify(&self) -> Result<(), SubjectError> {
        self.signature.verify(&self.content)
    }
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
pub struct ApprovalResponse {
    pub appr_req_hash: DigestIdentifier,
    pub approved: bool,
}

impl HashId for ApprovalResponse {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self).map_err(|_| {
            SubjectError::SignatureCreationFails("HashId for ApprovalResponse Fails".to_string())
        })
    }
}

impl Signed<ApprovalResponse> {
    pub fn new(
        event_proposal_hash: DigestIdentifier,
        approved: bool,
        signature: Signature,
    ) -> Self {
        let content = ApprovalResponse {
            appr_req_hash: event_proposal_hash,
            approved,
        };
        Self { content, signature }
    }

    pub fn verify(&self) -> Result<(), SubjectError> {
        self.signature.verify(&self.content)
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, BorshSerialize, BorshDeserialize, PartialOrd,
)]
pub struct UniqueApproval {
    pub approval: Signed<ApprovalResponse>,
}

impl PartialEq for UniqueApproval {
    fn eq(&self, other: &Self) -> bool {
        self.approval.signature.signer == other.approval.signature.signer
    }
}

impl Hash for UniqueApproval {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.approval.signature.signer.hash(state);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum ApprovalState {
    Pending,
    Responded,
    Obsolete,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct ApprovalEntity {
    request: Signed<ApprovalRequest>,
    reponse: Option<Signed<ApprovalResponse>>,
    state: ApprovalState,
}
