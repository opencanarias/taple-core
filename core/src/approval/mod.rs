use serde::{Deserialize, Serialize};

use crate::{
    event_request::EventRequest, identifier::DigestIdentifier,
    signature::Signature, commons::models::{Acceptance, event_proposal::EventProposal},
};

pub(crate) mod error;
mod inner_manager;
mod manager;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ApprovalMessages {
    RequestApproval(EventProposal),
    EmitVote(EmitVote),
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
    acceptance: Acceptance,
}
