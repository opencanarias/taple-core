use crate::{commons::errors::SubjectError, governance::error::RequestError};
use thiserror::Error;
use crate::database::Error as DbError;

#[derive(Error, Debug)]
pub enum LedgerError {
    #[error("A channel has been closed")]
    ChannelClosed,
    #[error("Subject Not Found: {0}")]
    SubjectNotFound(String),
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
}
