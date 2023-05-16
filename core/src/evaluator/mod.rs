use serde::{Deserialize, Serialize};

use crate::{
    commons::models::event_preevaluation::EventPreEvaluation, identifier::DigestIdentifier,
    signature::Signature,
};

use self::errors::EvaluatorErrorResponses;
#[cfg(feature = "evaluation")]
pub mod compiler;

mod errors;
#[cfg(feature = "evaluation")]
mod manager;
#[cfg(feature = "evaluation")]
pub use manager::{EvaluatorAPI, EvaluatorManager};
#[cfg(feature = "evaluation")]
mod runner;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EvaluatorMessage {
    AskForEvaluation(EventPreEvaluation),
}

#[derive(Clone, Debug)]
pub enum EvaluatorResponse {
    AskForEvaluation(Result<(), EvaluatorErrorResponses>),
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
