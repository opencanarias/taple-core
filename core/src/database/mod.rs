mod db;
mod error;
mod leveldb;
mod wrapper_leveldb;
mod memory;

use serde::{de::DeserializeOwned, Serialize};

pub use error::Error;
pub use db::DB;
pub use self::leveldb::LevelDB;
pub use self::memory::MemoryManager;

pub trait DatabaseManager: Sync + Send {
    fn create_collection<V>(
        &self,
        identifier: &str,
    ) -> Box<dyn DatabaseCollection<InnerDataType = V>>
    where
        V: Serialize + DeserializeOwned + Sync + Send + 'static;
}

pub trait DatabaseCollection: Sync + Send {
    type InnerDataType: Serialize + DeserializeOwned + Sync + Send;

    fn put(&self, key: &str, data: Self::InnerDataType) -> Result<(), Error>;
    fn get(&self, key: &str) -> Result<Self::InnerDataType, Error>;
    fn del(&self, key: &str) -> Result<(), Error>;
    fn partition<'a>(
        &'a self,
        key: &str,
    ) -> Box<dyn DatabaseCollection<InnerDataType = Self::InnerDataType> + 'a>;
    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (String, Self::InnerDataType)> + 'a>;
    fn rev_iter<'a>(&'a self) -> Box<dyn Iterator<Item = (String, Self::InnerDataType)> + 'a>;
}
