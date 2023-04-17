use thiserror::Error;

use crate::governance::error::RequestError;

#[derive(Error, Clone, Debug)]
pub enum EventError {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Governance Error")]
    GovernanceError(#[from] RequestError),
    #[error("Crypto Error")]
    CryptoError(String),
    #[error("Cant send message. Channel closed")]
    ChannelClosed,
    #[error("Subject has already an Event Completing")]
    EventAlreadyInProgress,
    #[error("Subject for state request not found")]
    SubjectNotFound(String),
    #[error("Event phase doesn't match")]
    WrongEventPhase,
    #[error("Governance version of evaluator doesn't match with ours")]
    WrongGovernanceVersion,
    #[error("Evaluation in Creation Event")]
    EvaluationInCreationEvent,
    #[error("Error parsing json string: {0}")]
    ErrorParsingJsonString(String),
    #[error("Error parsing value")]
    ErrorParsingValue,
    #[error("Error applying patch json string: {0}")]
    ErrorApplyingPatch(String),
    #[error("Channel unnavaible")]
    ChannelError(#[from] crate::commons::errors::ChannelErrors),
    #[error("Subject Not Owned: {0}")]
    SubjectNotOwned(String),
}
