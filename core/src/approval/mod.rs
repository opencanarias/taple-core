use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::models::Acceptance,
    identifier::{DigestIdentifier, KeyIdentifier},
    signature::{Signature, Signed},
    request::EventRequest, ApprovalRequest, ValueWrapper,
};

use self::error::ApprovalErrorResponse;

pub(crate) mod error;
#[cfg(feature = "aproval")]
mod inner_manager;
#[cfg(feature = "aproval")]
pub(crate) mod manager;

#[derive(Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize)]
pub enum ApprovalMessages {
    RequestApproval(Signed<ApprovalRequest>),
    EmitVote(EmitVote),
    GetAllRequest,
    GetSingleRequest(DigestIdentifier),
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ApprovalPetitionData {
    pub subject_id: DigestIdentifier,
    pub sn: u64,
    pub governance_id: DigestIdentifier,
    pub governance_version: u64,
    pub hash_event_proporsal: DigestIdentifier,
    pub sender: KeyIdentifier,
    pub json_patch: ValueWrapper,
}

#[derive(Clone, Debug)]
pub enum ApprovalResponses {
    RequestApproval(Result<(), ApprovalErrorResponse>),
    EmitVote(Result<(), ApprovalErrorResponse>),
    GetAllRequest(Vec<ApprovalPetitionData>),
    GetSingleRequest(Result<ApprovalPetitionData, ApprovalErrorResponse>),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RequestApproval {
    request: Signed<EventRequest>,
    sn: u64,
    context_hash: DigestIdentifier,
    hash_new_state: DigestIdentifier,
    governance_id: DigestIdentifier,
    governance_version: u64,
    success: bool,
    approval_required: bool,
    json_patch: String,
    evaluator_signatures: Vec<Signature>,
    subject_signature: Signature,
}

#[derive(Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize)]
pub struct EmitVote {
    request_id: DigestIdentifier,
    acceptance: Acceptance,
}
