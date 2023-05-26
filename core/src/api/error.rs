//! Errors that may occur when interacting with a TAPLE node through its API

use crate::{event::errors::EventError, approval::error::ApprovalErrorResponse};
pub use crate::protocol::errors::EventCreationError;
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum APIInternalError {
    #[error("Channel unavailable")]
    ChannelError,
    #[error("Oneshot channel not available")]
    OneshotUnavailable,
    #[error("An error has ocurred during sign process")]
    SignError,
    #[error("Unexpect response received after manager request")]
    UnexpectedManagerResponse,
    #[error("Database error {0}")]
    DatabaseError(String)
}

/// Errors that may occur when using the TAPLE API
#[derive(Error, Debug, Clone)]
pub enum ApiError {
    #[error("{}", source)]
    EventCreationError {
        #[from]
        source: EventError,
    },
    // OLD
    /// An item of the protocol has not been found, for example, a subject
    #[error("{0} not found")]
    NotFound(String),
    /// An unexpected error has occurred
    #[error("Unexpected Response")]
    UnexpectedError,
    /// An error has occurred in the process of creating an event.
    // #[error("{}", source)]
    // EventCreationError {
    //     #[from]
    //     source: EventCreationError,
    // },
    /// An internal error has occurred
    #[error("An error has ocurred during approval execution. {}", source)]
    ApprovalInternalError {
        #[from]
        source: ApprovalErrorResponse,
    },
    /// Invalid parameters have been entered, usually identifiers.
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),
    /// An error occurred during a signature process
    #[error("Sign Process Failed")]
    SignError,
    /// No permissions required or possessed to vote on an event request.
    #[error("Vote not needed for request {0}")]
    VoteNotNeeded(String),
    #[error("Not enough permissions. {0}")]
    NotEnoughPermissions(String),
    #[error("A database error has ocurred at API module: {0}")]
    DatabaseError(String),
}
