use commons::{
    errors::{CryptoErrorEvent, SubjectError},
    models::state::LedgerState,
};
use governance::error::RequestError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("A channel has been closed")]
    ChannelClosed,
    #[error("Invalid Identifier")]
    InvalidIdentifier,
    #[error("Wrogn Channel")]
    WrongChannel,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum LedgerManagerError {
    #[error("A channel has been closed")]
    ChannelClosed,
    #[error("Subject Not Found")]
    SubjectNotFound,
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
    #[error("Error in subject Subject")]
    SubjectError(SubjectError),
    #[error("Error with Governance")]
    GovernanceError(RequestError),
}

#[derive(Debug, PartialEq, Clone)]
pub enum CryptoError {
    Conflict,
    InvalidSignature,
    InvalidHash,
    Event(CryptoErrorEvent),
}
