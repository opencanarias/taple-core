use thiserror::Error;

use crate::{commons::errors::ChannelErrors, protocol::errors::ProtocolErrors};

use crate::database::Error as DatabaseError;

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
    #[error("Event SN lower than last signed")]
    EventSnLowerThanLastSigned,
    #[error("Trying to sign same sn with different hash")]
    DifferentHashForEvent,
    #[error("Serializing Error")]
    SerializingError,
    #[error("Database Error")]
    DatabaseError,
}
