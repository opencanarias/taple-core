use crate::identifier::DigestIdentifier;

use super::errors::ExecutorErrorResponses;

mod context;
mod executor;
mod manager;

#[derive(Clone, Debug)]
pub enum RunnerMessages {
  ExecuteContract(ExecuteContract),
  Shutdown
}

#[derive(Clone, Debug)]
pub struct ExecuteContract {
  governance_id: DigestIdentifier,
  schema: String,
  state: String,
  event: String
}

#[derive(Clone, Debug)]
pub enum RunnerResponses {
  ExecuteContract(Result<ExecuteContractResponse, ExecutorErrorResponses>),
  Shutdown
}

#[derive(Clone, Debug)]
pub struct ExecuteContractResponse {
  json_patch: String,
}

