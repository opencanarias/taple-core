use thiserror::Error;

use crate::{commons::errors::ChannelErrors, commons::errors::ProtocolErrors};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum NotaryError {
    #[error("ProtocolErrors Error")]
    ProtocolErrors(#[from] ProtocolErrors),
    #[error("Channel Error")]
    ChannelError(#[from] ChannelErrors),
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
}
