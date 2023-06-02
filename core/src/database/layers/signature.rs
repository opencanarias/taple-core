use super::utils::{get_key, Element};
use crate::commons::models::event::ValidationProof;
use crate::signature::Signature;
use crate::{DbError};
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
    ) -> Result<(HashSet<Signature>, ValidationProof), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let signatures = self.collection.get(&key)?;
        Ok(bincode::deserialize::<(HashSet<Signature>, ValidationProof)>(&signatures).map_err(|_| {
            DbError::DeserializeError
        })?)
    }

    pub fn set_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let total_signatures = match self.collection.get(&key) {
            Ok(other) => {
                let (other, _) = bincode::deserialize::<(HashSet<Signature>, ValidationProof)>(&other).map_err(|_| {
                    DbError::SerializeError
                })?;
                signatures.union(&other).cloned().collect()
            },
            Err(DbError::EntryNotFound) => signatures,
            Err(error) => {
                return Err(error);
            }
        };
        let total_signatures = bincode::serialize(&(total_signatures, validation_proof)).map_err(|_| {
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
}
