use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum NetworkErrors {
    #[error("All liste_addr must sahre the same internet protocol")]
    ProtocolConflict,
    #[error("Uninitialized event sender")]
    UninitializedEventSender,
}
