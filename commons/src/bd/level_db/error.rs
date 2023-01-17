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
    #[error("No table selected. Call select_table first")]
    TableNotSelectedError,
    #[error("There was an attempt to update an unexistent entry in DB")]
    EntryNotFoundError,
    #[error("There was an attempt to insert in an already existent entry in DB")]
    EntryAlreadyExists,
}
