use crate::identifier::DigestIdentifier;

use super::errors::ExecutorErrorResponses;

mod context;
mod executor;
pub mod manager;

#[derive(Clone, Debug)]
pub struct ExecuteContract {
    pub(crate) governance_id: DigestIdentifier,
    pub(crate) schema: String,
    pub(crate) state: String,
    pub(crate) event: String,
}

#[derive(Clone, Debug)]
pub struct ExecuteContractResponse {
    pub json_patch: String,
    pub hash_new_state: DigestIdentifier,
    pub governance_version: u64
}
