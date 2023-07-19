//! Possible errors of a TAPLE Database
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum Error {
    #[error("Entry Not Found")]
    EntryNotFound,
    #[error("Error while serializing")]
    SerializeError,
    #[error("Error while deserializing")]
    DeserializeError,
    #[error("Subject Apply failed")]
    SubjectApplyFailed,
    #[error("Conversion to Digest Identifier failed")]
    NoDigestIdentifier,
    #[error("Key Elements must have more than one element")]
    KeyElementsError,
    #[error("An error withing the database custom implementation {0}")]
    CustomError(String),
    #[error("State non existent, possibilities are: Pending or Voted.")]
    NonExistentStatus,
}
