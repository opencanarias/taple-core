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
}