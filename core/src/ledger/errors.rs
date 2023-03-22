use crate::commons::{
    errors::{CryptoErrorEvent, SubjectError},
    models::state::LedgerState,
};
use crate::governance::error::RequestError;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum LedgerManagerError {
    #[error("A channel has been closed")]
    ChannelClosed,
    #[error("Subject Not Found")]
    SubjectNotFound,
    #[error("Signatures not found")]
    SignaturesNotFound,
    #[error("Event Not Found")]
    EventNotFound(LedgerState),
    #[error("Event Not Needed")]
    EventNotNeeded(LedgerState),
    #[error("Event Already Exists")]
    EventAlreadyExists,
    #[error("Not Validable (We are not validator od subject)")]
    NotValidable,
    #[error("Cryptographic Error")]
    CryptoError(CryptoError),
    #[error("Head Candidate Not Validated")]
    HeadCandidateNotValidated,
    #[error("Multiple Events target in same bunch of signatures")]
    MultipleTargets,
    #[error("HashSet of Signatures is empty")]
    EmptySignatures,
    #[error("HashSet of Signatures has invalid validator")]
    InvalidValidator,
    #[error("Signatures not needed")]
    SignaturesNotNeeded,
    #[error("The error \"{0}\" has been generated during subject manipulation")]
    SubjectError(SubjectError),
    #[error("Error \"{0}\" detected with governance")]
    GovernanceError(RequestError),
    #[error("A database error has ocurred at LedgerManager: \"{0}\"")]
    DatabaseError(String)
}

#[derive(Debug, PartialEq, Clone)]
pub enum CryptoError {
    Conflict,
    InvalidSignature,
    InvalidHash,
    Event(CryptoErrorEvent),
}
