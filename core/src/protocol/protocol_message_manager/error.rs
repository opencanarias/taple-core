use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ProtocolErrors {
    #[error("Ask command not supported")]
    AskCommandDetected,
    #[error("Channel closes")]
    ChannelClosed,
}
