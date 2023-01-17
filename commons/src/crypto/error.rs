/*use ed25519_dalek::ed25519;
use std::convert::Infallible;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Errors that can never happen")]
    InfalibleError {
        #[from]
        source: Infallible,
    },

    #[error("Unknown error: `{0}`")]
    UnknownError(String),

    #[error("Verification error: {0}")]
    VerificationError(String),

    #[error("`{0}`")]
    PayloadError(String),

    #[error("Deserialization error")]
    DeserializationError,

    #[error("Base58 Decoding error")]
    Base64DecodingError {
        #[from]
        source: base64::DecodeError,
    },

    #[error("Ed25519 error")]
    Ed25519Error {
        #[from]
        source: ed25519::Error,
    },

    #[error("Serde JSON error")]
    SerdeJson {
        #[from]
        source: serde_json::Error,
    },

    #[error("Event error: {0}")]
    EventError(String),

    #[error("Seed error: {0}")]
    SeedError(String),

    #[error("Semantic error: {0}")]
    SemanticError(String),

    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),

    #[error("Sign error: {0}")]
    SignError(String),

    #[error("No signature error: {0}")]
    NoSignatureError(String),

    #[error("Key pair error: {0}")]
    KeyPairError(String),

    #[error("TAPLE error: {0}")]
    TapleError(String),

    #[error("Store error: {0}")]
    StoreError(String),

    #[error("Duplicate Event")]
    DuplicateEvent,

    #[error("Event out of order")]
    OutOfOrder,

    #[error("Schema not found")]
    SchemaNotFoundError,

    #[error("Subject not found")]
    SubjectNotFoundError,
}*/
