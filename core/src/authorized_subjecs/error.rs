use crate::database::Error as DbError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AuthorizedSubjectsError {
    #[error("Channel unnavaible")]
    ChannelError(#[from] crate::commons::errors::ChannelErrors),
    #[error("Database Error")]
    DatabaseError(#[from] DbError),
}
