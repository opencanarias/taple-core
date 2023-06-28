use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    identifier::{DigestIdentifier, KeyIdentifier},
    signature::{Signature, Signed},
    request::EventRequest, ApprovalRequest, ValueWrapper, commons::models::approval::ApprovalEntity,
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

#[derive(Clone, Debug)]
pub enum ApprovalResponses {
    RequestApproval(Result<(), ApprovalErrorResponse>),
    EmitVote(Result<(), ApprovalErrorResponse>),
    GetAllRequest(Vec<ApprovalEntity>),
    GetSingleRequest(Result<ApprovalEntity, ApprovalErrorResponse>),
}

#[derive(Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize)]
pub struct EmitVote {
    request_id: DigestIdentifier,
    acceptance: bool,
}
