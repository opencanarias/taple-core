use crate::database::DatabaseError as DbError;
use crate::{commons::errors::SubjectError, governance::error::RequestError};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum LedgerError {
    #[error("A channel has been closed")]
    ChannelClosed,
    #[error("Unexpected response")]
    UnexpectedResponse,
    #[error("Subject Not Found: {0}")]
    SubjectNotFound(String),
    #[error("Unexpected transfer received")]
    UnexpectedTransfer,
    #[error("Error parsing json string: \"{0}\"")]
    ErrorParsingJsonString(String),
    #[error("Error applying patch: \"{0}\"")]
    ErrorApplyingPatch(String),
    #[error("State Event entered as Genesis")]
    StateInGenesis,
    #[error("Channel unnavaible")]
    ChannelError(#[from] crate::commons::errors::ChannelErrors),
    #[error("Subject Error")]
    SubjectError(#[from] SubjectError),
    #[error("Crypto Error: \"{0}\"")]
    CryptoError(String),
    #[error("Subject ALready Exists: \"{0}\"")]
    SubjectAlreadyExists(String),
    #[error("Governance Error")]
    GovernanceError(#[from] RequestError),
    #[error("Database Error")]
    DatabaseError(#[from] DbError),
    #[error("Event Already Exists")]
    EventAlreadyExists,
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Not Enough Signatures: {0}")]
    NotEnoughSignatures(String),
    #[error("0 events for subject: {0}")]
    ZeroEventsSubject(String),
    #[error("Wrong SN in Subject: {0}")]
    WrongSnInSubject(String),
    #[error("LCE bigger than last LCE, so we ignore it")]
    LCEBiggerSN,
    #[error("Unsigned Unknown Event")]
    UnsignedUnknowEvent,
    #[error("Unsigned Unknown Event")]
    UnexpectEventMissingInEventSourcing,
    #[error("Event Not Next")]
    EventNotNext,
    #[error("Event Does Not Fit Hash")]
    EventDoesNotFitHash,
    #[error("We are not witnesses: {0}")]
    WeAreNotWitnesses(String),
    #[error("Invalid LCE After Genesis: {0}")]
    InvalidLCEAfterGenesis(String),
    #[error("Governance Not Preauthorized: {0}")]
    GovernanceNotPreauthorized(String),
    #[error("Governance LCE: {0}")]
    GovernanceLCE(String),
    #[error("Evaluation found in Transfer Event")]
    EvaluationInTransferEvent,
    #[error("Approval found in transfer event")]
    ApprovalInTransferEvent,
    #[error("State event with an SN of 0 detected")]
    StateEventWithZeroSNDetected,
    #[error("Unexpected create event")]
    UnexpectedCreateEvent,
    #[error("Validation Proof Error: {0}")]
    ValidationProofError(String),
    #[error("EOL when active LCE for subject: {0}")]
    EOLWhenActiveLCE(String),
    #[error("Intermediate EOL for subject: {0}")]
    IntermediateEOL(String),
    #[error("Subject Life Ended: {0}")]
    SubjectLifeEnd(String),
    #[error("Repeated Request ID: {0}")]
    RepeatedRequestId(String),
    #[error("Subject Id generation does not match with event subject_id")]
    SubjectIdError,
    #[error("Notification Channel Error")]
    NotificationChannelError,
}
