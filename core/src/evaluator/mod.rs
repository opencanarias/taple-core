use crate::{identifier::DigestIdentifier, event_request::EventRequest, signature::Signature};

use self::errors::EvaluatorErrorResponses;

mod compiler;
mod manager;
mod errors;
mod runner;

#[derive(Clone, Debug)]
pub enum EvaluatorMessage {
  AskForEvaluation(AskForEvaluation)
}

#[derive(Clone, Debug)]
pub enum EvaluatorResponse {
  AskForEvaluation(Result<AskForEvaluationResponse, EvaluatorErrorResponses>)
}

#[derive(Clone, Debug)]
pub struct AskForEvaluation {
  governance_id: DigestIdentifier,
  schema_id: String,
  state: String,
  invokation: EventRequest,
}

#[derive(Clone, Debug)]
pub struct AskForEvaluationResponse {
  governance_version: u64,
  hash_new_state: String,
  json_patch: String,
  signature: Signature,
}
