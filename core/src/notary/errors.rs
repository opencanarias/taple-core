use thiserror::Error;

use crate::{
    commons::errors::ChannelErrors, commons::errors::ProtocolErrors,
    governance::error::RequestError,
};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum NotaryError {
    #[error("ProtocolErrors Error")]
    ProtocolErrors(#[from] ProtocolErrors),
    #[error("Channel Error")]
    ChannelError(#[from] ChannelErrors),
    #[error("Governance Error: {0}")]
    GovernanceError(#[from] RequestError),
    #[error("Input Channel Error")]
    InputChannelError,
    #[error("Owner Not Known")]
    OwnerSubjectNotKnown,
    #[error("Governance Id Not Found")]
    GovernanceNotFound,
    #[error("Governance API unexpected response")]
    GovApiUnexpectedResponse,
    #[error("Governance Version too High")]
    GovernanceVersionTooHigh,
    #[error("Governance Version too Low")]
    GovernanceVersionTooLow,
    #[error("Event SN lower than last signed")]
    EventSnLowerThanLastSigned,
    #[error("Trying to sign same sn with different Proof")]
    DifferentProofForEvent,
    #[error("Serializing Error")]
    SerializingError,
    #[error("Database Error")]
    DatabaseError,
    #[error("Previuous Proof Left")]
    PreviousProofLeft,
    #[error("Invalid Signature")]
    InvalidSignature,
    #[error("Invalid Signer")]
    InvalidSigner,
    #[error("Quorum Not Reached")]
    QuorumNotReached,
    #[error("Subject Signature Not Valid")]
    SubjectSignatureNotValid,
    #[error("Diferent genesis_gov_version and gov_version for subject: {0}")]
    GenesisGovVersionsDoesNotMatch(String),

}
