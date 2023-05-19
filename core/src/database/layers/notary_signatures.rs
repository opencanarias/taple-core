use super::utils::{get_key, Element};
use crate::commons::models::notary::NotaryEventResponse;
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) struct NotarySignaturesDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> NotarySignaturesDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("notary-signatures"),
            prefix: "notary-signatures".to_string(),
        }
    }

    pub fn get_notary_signatures(
        &self,
        subject_id: &DigestIdentifier,
        _sn: u64,
    ) -> Result<(u64, HashSet<NotaryEventResponse>), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let notary_signatures = self.collection.get(&key)?;
        Ok(
            bincode::deserialize::<(u64, HashSet<NotaryEventResponse>)>(&notary_signatures)
            .map_err(|_| {
                DbError::DeserializeError
            })?,
        )
    }

    pub fn set_notary_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<NotaryEventResponse>,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let total_signatures = match self.collection.get(&key) {
            Ok(other) => {
                let other =
                    bincode::deserialize::<(u64, HashSet<NotaryEventResponse>)>(&other).map_err(|_| {
                        DbError::DeserializeError
                    })?;
                signatures.union(&other.1).cloned().collect()
            }
            Err(DbError::EntryNotFound) => signatures,
            Err(error) => {
                return Err(error);
            }
        };
        let Ok(data) = bincode::serialize::<(u64, HashSet<NotaryEventResponse>)>(&(sn, total_signatures)) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn delete_notary_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}
