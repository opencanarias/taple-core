use thiserror::Error;

use tokio::task::JoinError;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Subject Task error")]
    TaskError {
        #[from]
        source: JoinError,
    },
    #[error("Sender Channel Error")]
    SenderChannelError,
    #[error("Serde JSON error")]
    SerdeJson {
        #[from]
        source: serde_json::Error,
    },
    #[error("Serde CBOR error")]
    SerdeCbor {
        #[from]
        source: serde_cbor::Error,
    },
    #[error("MessagePack serialize error")]
    MsgPackSerialize {
        #[from]
        source: rmp_serde::encode::Error,
    },

    #[error("MessagePack deserialize error")]
    MsgPackDeserialize {
        #[from]
        source: rmp_serde::decode::Error,
    },
    #[error("Cant send message. Channel closed")]
    ChannelClosed,
    #[error("Error Creating message")]
    CreatingMessageError,
}
