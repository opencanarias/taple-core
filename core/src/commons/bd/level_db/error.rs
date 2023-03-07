use leveldb::database;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WrapperLevelDBErrors {
    #[error("Internal LevelDB::Error")]
    LevelDBError {
        #[from]
        source: database::error::Error,
    },
    #[error("Error while serializing")]
    SerializeError,
    #[error("Error while deserializing")]
    DeserializeError,
    #[error("There was an attempt to update an unexistent entry in DB")]
    EntryNotFoundError,
}
