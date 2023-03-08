use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum RequestError {
    #[error("Governance requested not found")]
    GovernanceNotFound(String),
    #[error("Subject requested not found")]
    SubjectNotFound,
    #[error("Schema requested not found")]
    SchemaNotFound(String),
    #[error("JSON Schema compile error")]
    JSONCompileError,
    #[error("Invalid KeyIdentifier {0}")]
    InvalidKeyIdentifier(String),
    #[error("Unexpected response")]
    UnexpectedResponse,
    #[error("Channel Closed")]
    ChannelClosed,
    #[error("Invalid Request Type")]
    InvalidRequestType,
    #[error("Schema Not Found in policies")]
    SchemaNotFoundInPolicies,
    #[error("The specified governance ID is of a subject")]
    InvalidGovernanceID,
    #[error("Unexpect Payload")]
    UnexpectedPayloadType
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
    #[error("Invalid KeyIdentifier")]
    InvalidGovernancePayload,
}
