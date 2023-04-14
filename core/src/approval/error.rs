use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ApprovalManagerError {
  #[error("Ask not allowed for this module")]
  AskNoAllowed,
}

#[derive(Error, Debug, Clone)]
pub enum ApprovalErrorResponse {

}