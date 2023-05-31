use thiserror::Error;

use crate::{commons::errors::SubjectError, governance::error::RequestError};

#[derive(Error, Clone, Debug)]
pub enum EventError {
    #[error("Event API channel not available")]
    EventApiChannelNotAvailable,
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Governance Error")]
    GovernanceError(#[from] RequestError),
    #[error("Subject Error: {0}")]
    SubjectError(#[from] SubjectError),
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
    EvaluationOrApprovationInCreationEvent,
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
    #[error("External Genesis Event")]
    ExternalGenesisEvent,
    #[error("Creating Permission Denied")]
    CreatingPermissionDenied,
    #[error("Genesis In Gov Update:")]
    GenesisInGovUpdate,
    #[error("Transfer events are not evaluated")]
    NoEvaluationForTransferEvents,
    #[error("Transfer events are not approved")]
    NoAprovalForTransferEvents
}
