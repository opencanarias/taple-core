use super::utils::{get_key, Element};
use crate::signature::Signature;
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) struct SignatureDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> SignatureDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("signature"),
            prefix: "signature".to_string(),
        }
    }

    pub fn get_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Result<HashSet<Signature>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let signatures = self.collection.get(&key)?;
        Ok(bincode::deserialize::<HashSet<Signature>>(&signatures).map_err(|_| {
            DbError::DeserializeError
        })?)
    }

    pub fn set_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let total_signatures = match self.collection.get(&key) {
            Ok(other) => {
                let other = bincode::deserialize::<HashSet<Signature>>(&other).map_err(|_| {
                    DbError::DeserializeError
                })?;
                signatures.union(&other).cloned().collect()
            },
            Err(DbError::EntryNotFound) => signatures,
            Err(error) => {
                return Err(error);
            }
        };
        let total_signatures = bincode::serialize(&total_signatures).map_err(|_| {
            DbError::SerializeError
        })?;
        self.collection.put(&key, total_signatures)
    }

    pub fn del_signatures(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }

    pub fn get_validation_proof(&self, subject_id: &DigestIdentifier) -> Result<HashSet<Signature>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let mut iter = self.collection.iter(false, format!("{}{}", key, char::MAX));
        if let Some(vproof) = iter.next() {
            let vproof = bincode::deserialize::<HashSet<Signature>>(&vproof.1).map_err(|_| {
                DbError::DeserializeError
            })?;;
            return Ok(vproof);
        } else {
            return Err(DbError::EntryNotFound)
        }
    }
}
