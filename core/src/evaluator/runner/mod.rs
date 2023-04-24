use crate::{identifier::DigestIdentifier, commons::models::Acceptance};

mod context;
mod executor;
pub mod manager;

#[derive(Clone, Debug)]
pub struct ExecuteContractResponse {
    pub json_patch: String,
    pub hash_new_state: DigestIdentifier,
    pub context_hash: DigestIdentifier,
    pub governance_version: u64,
    pub success: Acceptance,
    pub approval_required: bool,
}

