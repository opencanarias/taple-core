use super::utils::{get_key, Element};
use crate::crypto::KeyPair;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use crate::DbError;
use std::sync::Arc;


pub(crate) struct TransferEventsDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> TransferEventsDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("transfer"),
            prefix: "transfer".to_string(),
        }
    }

    pub fn get_expecting_transfer(&self, subject_id: &DigestIdentifier) -> Result<KeyPair, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let value = self.collection.get(&key)?;
        let result = bincode::deserialize::<KeyPair>(&value)
            .map_err(|_| DbError::DeserializeError)?;
        Ok(result)
    }

    pub fn set_expecting_transfer(
        &self,
        subject_id: &DigestIdentifier,
        keypair: KeyPair,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<KeyPair>(&keypair) else {
            return Err(DbError::SerializeError);
        }; 
        self.collection.put(&key, data)
    }

    pub fn del_expecting_transfer(&self, subject_id: &DigestIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}