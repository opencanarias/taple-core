use thiserror::Error;

use crate::{commons::errors::SubjectError, governance::error::RequestError};

#[derive(Error, Clone, Debug)]
pub enum EventError {
    #[error("Event API channel not available")]
    EventApiChannelNotAvailable,
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Governance Error: {0}")]
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
    NoAprovalForTransferEvents,
    #[error("EOL events are not evaluated")]
    NoEvaluationForEOLEvents,
    #[error("EOL events are not approved")]
    NoAprovalForEOLEvents,
    #[error("KeyID: {0}, not authorized for close")]
    CloseNotAuthorized(String),
    #[error("Subject Life Ended: {0}")]
    SubjectLifeEnd(String),
    #[error("Invoke permission denied for ID: {0}, Subject ID: {1}")]
    InvokePermissionDenied(String, String),
    #[error("Hash generation failed")]
    HashGenerationFailed,
    #[error("Request already known")]
    RequestAlreadyKnown,
    #[error("Subject Keys Not Found")]
    SubjectKeysNotFound(String),
    #[error("Event0 Not Create")]
    Event0NotCreate,
    #[error("Own Transfer Keys Db Error")]
    OwnTransferKeysDbError,
}
