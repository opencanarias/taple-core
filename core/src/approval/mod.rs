use serde::{Serialize, Deserialize};

use crate::{identifier::{DigestIdentifier, KeyIdentifier}, TimeStamp, signature::Signature, message::TaskCommandContent, event_request::EventRequest};

mod error;
mod manager;
mod inner_manager;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ApprovalMessages {
  RequestApproval(RequestApproval)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RequestApproval {
  request: EventRequest,  
  sn: u64,
  context_hash: DigestIdentifier,
  hash_new_state: DigestIdentifier,
  success: bool,
  approval_required: bool,
  evaluator_signature: Signature,
  json_patch: String, 
}

impl TaskCommandContent for ApprovalMessages {}
