mod db;
mod error;
mod memory;

pub use self::memory::MemoryManager;
pub use db::DB;
pub use error::Error;

pub trait DatabaseManager<C>: Sync + Send
where
    C: DatabaseCollection,
{
    fn create_collection(&self, identifier: &str) -> C;
}

pub trait DatabaseCollection: Sync + Send {
    fn get(&self, key: &str) -> Result<Vec<u8>, Error>;
    fn put(&self, key: &str, data: Vec<u8>) -> Result<(), Error>;
    fn del(&self, key: &str) -> Result<(), Error>;
    fn iter<'a>(
        &'a self,
        reverse: bool,
        prefix: String,
    ) -> Box<dyn Iterator<Item = (String, Vec<u8>)> + 'a>;
}
