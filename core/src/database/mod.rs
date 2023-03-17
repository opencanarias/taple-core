mod db;
mod error;
mod leveldb;
mod wrapper_leveldb;

use std::marker::PhantomData;

use serde::{de::DeserializeOwned, Serialize};

use error::Error;

pub trait DatabaseManager {
    fn create_collection<V>(
        &self,
        identifier: &str,
    ) -> Box<dyn DatabaseCollection<InnerDataType = V>>
    where
        V: Serialize + DeserializeOwned + 'static;
}

pub struct KeyData<'a> {
    subject_id: Option<&'a str>,
    sn: Option<u64>,
}

pub trait DatabaseCollection {
    type InnerDataType: Serialize + DeserializeOwned;

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
