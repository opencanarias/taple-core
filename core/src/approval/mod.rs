use serde::{Deserialize, Serialize};

use crate::{
    event_request::EventRequest, identifier::DigestIdentifier, message::TaskCommandContent,
    signature::Signature, Acceptance,
};

mod error;
mod inner_manager;
mod manager;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ApprovalMessages {
    RequestApproval(RequestApproval),
    EmitVote(EmitVote)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RequestApproval {
    request: EventRequest,
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EmitVote {
  request_id: DigestIdentifier,
  acceptance: Acceptance
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct VoteMessage {
    event_proposal_hash: DigestIdentifier,
    acceptance: Acceptance,
    signature: Signature,
}

impl TaskCommandContent for VoteMessage {}
