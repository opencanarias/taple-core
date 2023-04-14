use borsh::BorshSerialize;

use crate::{event_request::EventRequest, identifier::{DigestIdentifier, KeyIdentifier}, signature::Signature};

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
    invokation: EventRequest, // Event
    hash_request: String,
    context: Context,
    state: String,
    sn: u64
}

#[derive(Clone, Debug, BorshSerialize)]
pub struct Context {
    governance_id: DigestIdentifier,
    schema_id: String,
    invokator: KeyIdentifier,
    creator: KeyIdentifier,
    owner: KeyIdentifier,
    namespace: String,
}

#[derive(Clone, Debug)]
pub struct AskForEvaluationResponse {
    pub governance_version: u64,
    pub hash_new_state: DigestIdentifier,
    pub json_patch: String,
    pub success: bool,
    pub approval_required: bool,
    pub signature: Signature,
}
