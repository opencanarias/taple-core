use thiserror::Error;

use crate::commons::errors::ChannelErrors;

#[derive(Error, Debug, Clone)]
pub enum NotaryError {
    #[error("Channel Error")]
    ChannelError(#[from] ChannelErrors),
    #[error("Input Channel Error")]
    InputChannelError,
    #[error("Owner Not Known")]
    OwnerNotKnown,
    #[error("Governance Id Not Found")]
    GovernanceNotFound,
    #[error("Governance API unexpected response")]
    GovApiUnexpectedResponse,
}
