use thiserror::Error;

#[derive(Error, Debug, Clone)]
#[allow(dead_code)]
pub enum DistributionManagerError {
    #[error("Tell not allowed for this module")]
    TellNoAllowed,
    #[error("Governance channel not available")]
    GovernanceChannelNotAvailable,
    #[error("Database mismatch. The event specified by the subject SN does not exist")]
    DatabaseMismatch,
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Unexpected error")]
    UnexpectedError,
    #[error("Hash generation failed")]
    HashGenerationFailed,
    #[error("Sign generation failed")]
    SignGenerarionFailed,
    #[error("Message sender channel not available")]
    MessageChannelNotAvailable,
    #[error("Subject creation error")]
    SubjectCreationError,
    #[error("Response channel not available")]
    ResponseChannelNotAvailable,
}

#[derive(Error, Debug, Clone)]
pub enum DistributionErrorResponses {
    #[error("The node is not a witness of subject")]
    NoValidWitness,
    #[error("Channel not available")]
    ChannelNotAvailable,
    #[error("The event is not signed with the cryptographic material of the subject")]
    InvalidSubjectSignature,
    #[error("The previous hash of the event does not link with previous registered event")]
    InvalidEventLink,
    #[error("Invalid signatures at the event")]
    InvalidEventSignatures,
    #[error("Invalid invokator of event")]
    InvalidInvokator,
    #[error("Invalid request type")]
    InvalidRequestType,
    #[error("Governance {0} not found")]
    GovernanceNotFound(String),
    #[error("Invalid Key identifier detected: {0}")]
    InvalidKeyIdentifier(String),
    #[error("The event has not reached approval quorum")]
    ApprovalQuorumNotReached,
    #[error("Approval quorum mismatch")]
    ApprovalQuorumMismatch,
    #[error("Invalid validation signatures")]
    InvalidValidationSignatures,
    #[error("Schema {0} not found")]
    SchemaNotFound(String),
    #[error("Event {0} of subject {1} not found")]
    EventNotFound(u64, String),
    #[error("Subject not found")]
    SubjectNotFound,
    #[error("Signatures not found")]
    SignaturesNotFound,
    #[error("Event without validator signatures received")]
    NoValidatorSignatures,
    #[error("Invalid Validators signatures hash")]
    InvalidValidatorSignatureHash,
    #[error("Event not needed")]
    EventNotNeeded,
    #[error("Event without validator signatures")]
    InvalidEvent,
    #[error("Signatures not needed")]
    SignatureNotNeeded,
    #[error("Invalid evaluator signatures")]
    InvalidEvaluatorSignatures,
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid signer")]
    InvalidSigner,
    #[error("Invalid DigestIdentifier")]
    InvalidDigestIdentifier,
}
