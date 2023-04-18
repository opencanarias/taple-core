use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ApprovalManagerError {
  #[error("Ask not allowed for this module")]
  AskNoAllowed,
  #[error("Governance channel failed")]
  GovernanceChannelFailed,
  #[error("Unexpected error")]
  UnexpectedError,
  #[error("Database error")]
  DatabaseError,
  #[error("Hash generation failed")]
  HashGenerationFailed,
  #[error("Sign process failed")]
  SignProcessFailed
}

#[derive(Error, Debug, Clone)]
pub enum ApprovalErrorResponse {
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
}