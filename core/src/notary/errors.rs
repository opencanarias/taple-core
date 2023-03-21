use thiserror::Error;

use crate::commons::errors::ChannelErrors;

#[derive(Error, Debug, Clone)]
pub enum NotaryError {
    #[error("Channel Error")]
    ChannelError(#[from] ChannelErrors),
    #[error("Input Channel Error")]
    InputChannelError,
}
