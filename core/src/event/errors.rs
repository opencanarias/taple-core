use thiserror::Error;

use crate::governance::error::RequestError;

#[derive(Error, Debug)]
pub enum EvaluatorError {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String), 
    #[error("Governance Error")]
    GovernanceError(#[from] RequestError),
}