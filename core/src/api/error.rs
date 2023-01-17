//! Errors that may occur when interacting with a TAPLE node through its API

use protocol::errors::{EventCreationError, ResponseError};
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum APIInternalError {
    #[error("Channel unavailable")]
    ChannelError {
        #[from]
        source: commons::errors::ChannelErrors,
    },
    #[error("Oneshot channel not available")]
    OneshotUnavailable,
}

/// Errors that may occur when using the TAPLE API
#[derive(Error, Debug, Clone)]
pub enum ApiError {
    /// An item of the protocol has not been found, for example, a subject
    #[error("Not Found: {0} ")]
    NotFound(String),
    /// An unexpected error has occurred
    #[error("Unexpected Response")]
    UnexpectedError,
    /// An error has occurred in the process of creating an event.
    #[error("Event Request could not be completed")]
    EventCreationError {
        #[from]
        source: EventCreationError,
    },
    /// An internal error has occurred
    #[error("Internal Error")]
    InternalError {
        #[from]
        source: ResponseError,
    },
    /// Invalid parameters have been entered, usually identifiers.
    #[error("Invalid parameters")]
    InvalidParameters,
    /// An error occurred during a signature process
    #[error("Sign Process Failed")]
    SignError,
    /// No permissions required or possessed to vote on an event request.
    #[error("Vote not Needed {0}")]
    VoteNotNeeded(String),
}
