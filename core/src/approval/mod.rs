use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::models::approval::ApprovalEntity,
    identifier::{DigestIdentifier, KeyIdentifier},
    request::EventRequest,
    signature::{Signature, Signed},
    ApprovalRequest,
};

use self::error::ApprovalErrorResponse;

pub(crate) mod error;
#[cfg(feature = "approval")]
mod inner_manager;
#[cfg(feature = "approval")]
pub(crate) mod manager;

#[derive(Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize)]
pub enum ApprovalMessages {
    RequestApproval(Signed<ApprovalRequest>),
    RequestApprovalWithSender {
        approval: Signed<ApprovalRequest>,
        sender: KeyIdentifier,
    },
    EmitVote(EmitVote),
    GetAllRequest,
    GetSingleRequest(DigestIdentifier),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum ApprovalResponses {
    RequestApproval(Result<(), ApprovalErrorResponse>),
    EmitVote(Result<ApprovalEntity, ApprovalErrorResponse>),
    GetAllRequest(Vec<ApprovalEntity>),
    GetSingleRequest(Result<ApprovalEntity, ApprovalErrorResponse>),
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
    acceptance: bool,
}
