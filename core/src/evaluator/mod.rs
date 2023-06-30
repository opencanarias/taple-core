use borsh::{BorshSerialize, BorshDeserialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::models::evaluation::EvaluationRequest, identifier::DigestIdentifier,
    signature::Signature, ValueWrapper, KeyIdentifier,
};

use self::errors::EvaluatorErrorResponses;
#[cfg(feature = "evaluation")]
pub mod compiler;

mod errors;
#[cfg(feature = "evaluation")]
mod manager;
#[cfg(feature = "evaluation")]
pub use manager::{EvaluatorManager};
#[cfg(feature = "evaluation")]
mod runner;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum EvaluatorMessage {
    EvaluationEvent {
        evaluation_request: EvaluationRequest,
        sender: KeyIdentifier,
    },
    AskForEvaluation(EvaluationRequest),
}

#[derive(Clone, Debug)]
pub enum EvaluatorResponse {
    AskForEvaluation(Result<(), EvaluatorErrorResponses>),
}
#[derive(Clone, Debug)]
pub struct AskForEvaluationResponse {
    pub governance_version: u64,
    pub hash_new_state: DigestIdentifier,
    pub json_patch: ValueWrapper,
    pub success: bool,
    pub approval_required: bool,
    pub signature: Signature,
}
