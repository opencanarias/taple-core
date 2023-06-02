use super::utils::{get_key, Element};
use crate::crypto::KeyPair;
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier, KeyIdentifier};
use std::collections::HashSet;
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

    pub fn get_expecting_transfer(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<KeyPair, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let value = self.collection.get(&key)?;
        let result =
            bincode::deserialize::<KeyPair>(&value).map_err(|_| DbError::DeserializeError)?;
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

    pub fn get_all_expecting_transfers(
        &self,
    ) -> Result<Vec<(DigestIdentifier, HashSet<KeyIdentifier>)>, DbError> {
        let key_elements: Vec<Element> = vec![Element::S(self.prefix.clone())];
        let key = get_key(key_elements)?;
        let iter = self.collection.iter(false, key);
        let mut result = Vec::new();
        for (_, bytes) in iter {
            let subject =
                bincode::deserialize::<(DigestIdentifier, HashSet<KeyIdentifier>)>(&bytes)
                    .map_err(|_| DbError::DeserializeError)?;
            result.push(subject);
        }
        Ok(result)
    }
}
