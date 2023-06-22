use crate::utils::{deserialize, serialize};
use super::utils::{get_key, Element};
use crate::crypto::KeyPair;
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier, KeyIdentifier};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) struct KeysDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> KeysDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("transfer"),
            prefix: "keys".to_string(),
        }
    }

    pub fn get_keys(&self, public_key: &KeyIdentifier) -> Result<KeyPair, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(public_key.to_str()),
        ];
        let key = get_key(key_elements)?;
        let value = self.collection.get(&key)?;
        let result =
            deserialize::<KeyPair>(&value).map_err(|_| DbError::DeserializeError)?;
        Ok(result)
    }

    pub fn set_keys(&self, public_key: &KeyIdentifier, keypair: KeyPair) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(public_key.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<KeyPair>(&keypair) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_keys(&self, public_key: &KeyIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(public_key.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }

    pub fn get_all_keys(&self) -> Result<Vec<KeyPair>, DbError> {
        let key_elements: Vec<Element> = vec![Element::S(self.prefix.clone())];
        let key = get_key(key_elements)?;
        let iter = self.collection.iter(false, key);
        let mut result = Vec::new();
        for (_, bytes) in iter {
            let subject =
                deserialize::<KeyPair>(&bytes)
                    .map_err(|_| DbError::DeserializeError)?;
            result.push(subject);
        }
        Ok(result)
    }
}
