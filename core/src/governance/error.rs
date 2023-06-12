use crate::{commons::errors::SubjectError, database::Error as DbError};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum RequestError {
    #[error("Database Error")]
    DatabaseError(#[from] DbError),
    #[error("Subject Error")]
    SubjectError(#[from] SubjectError),
    #[error("Governance requested not found")]
    GovernanceNotFound(String),
    #[error("Subject requested not found")]
    SubjectNotFound,
    #[error("Schema requested not found")]
    SchemaNotFound(String),
    #[error("JSON Schema compile error")]
    JSONCompileError,
    #[error("Error Parsing Json String")]
    ErrorParsingJsonString(String),
    #[error("Invalid KeyIdentifier {0}")]
    InvalidKeyIdentifier(String),
    #[error("Invalid Name {0}")]
    InvalidName(String),
    #[error("Unexpected response")]
    UnexpectedResponse,
    #[error("Channel Closed")]
    ChannelClosed,
    #[error("Governance version too high: {0}, {1}")]
    GovernanceVersionTooHigh(String, u64),
    #[error("Invalid Request Type")]
    InvalidRequestType,
    #[error("Schema Not Found in policies")]
    SchemaNotFoundInPolicies,
    #[error("The specified governance ID is of a subject")]
    InvalidGovernanceID,
    #[error("Unexpect Payload")]
    UnexpectedPayloadType,
    #[error("Searching signers quorum in wrong stage")]
    SearchingSignersQuorumInWrongStage(String),
    #[error("Searching invoke info in wrong stage")]
    SearchingInvokeInfoInWrongStage(String),
}

#[derive(Error, Debug)]
pub enum InternalError {
    #[error("Channel unnavaible")]
    ChannelError {
        #[from]
        source: crate::commons::errors::ChannelErrors,
    },
    #[error("Response Oneshot closed")]
    OneshotClosed,
    #[error("Deserialization error")]
    DeserializationError,
    #[error("Invalid KeyIdentifier: {0}")]
    InvalidGovernancePayload(String),
    #[error("Database error: {}", source)]
    DatabaseError {
        #[from]
        source: DbError,
    },
    #[error("Base 64 decode error")]
    Base64DecodingError,
}
