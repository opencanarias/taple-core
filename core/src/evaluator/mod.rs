use crate::{event_request::EventRequest, identifier::DigestIdentifier, signature::Signature};

use self::errors::EvaluatorErrorResponses;

pub mod compiler;
mod errors;
mod manager;
mod runner;

#[derive(Clone, Debug)]
pub enum EvaluatorMessage {
    AskForEvaluation(AskForEvaluation),
}

#[derive(Clone, Debug)]
pub enum EvaluatorResponse {
    AskForEvaluation(Result<AskForEvaluationResponse, EvaluatorErrorResponses>),
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
