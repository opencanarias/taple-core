use crate::{commons::errors::SubjectError, governance::error::RequestError};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum LedgerError {
    #[error("A channel has been closed")]
    ChannelClosed,
    #[error("Subject Not Found: {0}")]
    SubjectNotFound(String),
    #[error("Error \"{0}\" detected with governance")]
    GovernanceError(RequestError),
    #[error("A database error has ocurred at LedgerManager: \"{0}\"")]
    DatabaseError(String),
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
    SubjectAlreadyExists(String)
}
