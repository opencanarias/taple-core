//! Possible errors of a TAPLE Node
use config::ConfigError;
use thiserror::Error;

/// Possible errors that a TAPLE node can generate.
/// It does not include internal errors that may be produced by architecture modules.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Settings Load Error")]
    SettingsError {
        #[from]
        source: ConfigError,
    },
    #[error("Access Point parse error: {0}")]
    AcessPointError(String),
    #[error("API not yet avalabile")]
    ApiNotYetAvailable,
    #[error("Can't received anymore notifications. All notification senders dropped")]
    CantReceiveNotification,
    #[error("No notifications pending")]
    NoNewNotification,
    #[error("Can't generate PK. Both seed and explicit PK are defined in provided settings")]
    PkConflict,
    #[error("Either a seed or the MC private key must be specified to start a node")]
    NoMCAvailable,
    #[error("Invalid Hex String as Private Key")]
    InvalidHexString,
    #[error("Node has previously executed with a different KeyPair. Please, specify the same KeyPair as before. Current ControllerID {0}")]
    InvalidKeyPairSpecified(String),
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Serialization Error")]
    SerializeError,
    #[error("DeSerialization Error")]
    DeSerializeError,
}
