use crate::governance::error::RequestError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum LedgerError {
    #[error("A channel has been closed")]
    ChannelClosed,
    #[error("Subject Not Found")]
    SubjectNotFound,    
    #[error("Error \"{0}\" detected with governance")]
    GovernanceError(RequestError),
    #[error("A database error has ocurred at LedgerManager: \"{0}\"")]
    DatabaseError(String)
}
