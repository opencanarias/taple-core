use thiserror::Error;

use crate::{DigestIdentifier, KeyIdentifier};

#[derive(Error, Debug, Clone)]
#[allow(dead_code)]
pub enum ApprovalManagerError {
    #[error("Ask not allowed for this module")]
    AskNoAllowed,
    #[error("Governance channel failed")]
    GovernanceChannelFailed,
    #[error("Message channel failed")]
    MessageChannelFailed,
    #[error("Unexpected error")]
    UnexpectedError,
    #[error("Database error")]
    DatabaseError,
    #[error("Hash generation failed")]
    HashGenerationFailed,
    #[error("Sign process failed")]
    SignProcessFailed,
    #[error("Response channel closed")]
    ResponseChannelClosed,
    #[error("Invalid request type found")]
    InvalidRequestTypeFound,
    #[error("Unexpected request type found in database")]
    UnexpectedRequestType,
    #[error("More than one pending approval request detected")]
    MoreRequestThanMaxAllowed,
    #[error("Event Apply failed")]
    EventApplyFailed,
}

#[derive(Error, Debug, Clone)]
pub enum ApprovalErrorResponse {
    #[error("Evaluation is not present at request")]
    NotEvaluationInRequest,
    #[error("API Channel not available")]
    APIChannelNotAvailable,
    #[error("Request already known")]
    RequestAlreadyKnown,
    #[error("No Fact event")]
    NoFactEvent,
    #[error("Previous Event detected")]
    PreviousEventDetected,
    #[error("Governance not found")]
    GovernanceNotFound,
    #[error("Invalid governance ID")]
    InvalidGovernanceID,
    #[error("Invalid Governance version")]
    InvalidGovernanceVersion,
    #[error("Governance version is lower")]
    OurGovIsLower {
        our_id: KeyIdentifier,
        sender: KeyIdentifier,
        gov_id: DigestIdentifier,
    },
    #[error("Governance version is higher")]
    OurGovIsHigher {
        our_id: KeyIdentifier,
        sender: KeyIdentifier,
        gov_id: DigestIdentifier,
    },
    #[error("Subject not found")]
    SubjectNotFound,
    #[error("No correlation between governances id")]
    GovernanceNoCorrelation,
    #[error("Subject not synchronized")]
    SubjectNotSynchronized,
    #[error("Signature is not from suject")]
    SignatureSignerIsNotSubject,
    #[error("Invalid Subject signature")]
    InvalidSubjectSignature,
    #[error("Node is not an approver")]
    NodeIsNotApprover,
    #[error("Invalid evaluator detected")]
    InvalidEvaluator,
    #[error("Invalid Evaluator signature detected")]
    InvalidEvaluatorSignature,
    #[error("Invalid invokator signature")]
    InvalidInvokator,
    #[error("Incokator has no permission")]
    InvalidInvokatorPermission,
    #[error("No Evaluator Quroum reached")]
    NoQuorumReached,
    #[error("Approval request not found")]
    ApprovalRequestNotFound,
    #[error("No hash correlation")]
    NoHashCorrelation,
    #[error("Invalid acceptance")]
    InvalidAcceptance,
    #[error("Error Hashing")]
    ErrorHashing,
    #[error("Invalid state hash specified by request")]
    InvalidStateHashAfterApply,
    #[error("Request not found")]
    RequestNotFound,
    #[error("Request is not pending")]
    NotPendingRequest,
    #[error("Request already Responded")]
    RequestAlreadyResponded,
}
