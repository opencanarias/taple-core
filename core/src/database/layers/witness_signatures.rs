use super::utils::{get_key, Element};
use crate::signature::Signature;
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

pub(crate) struct WitnessSignaturesDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> WitnessSignaturesDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("witness-signatures"),
            prefix: "witness-signatures".to_string(),
        }
    }

    pub fn get_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<(u64, HashSet<Signature>), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let witness_signatures = self.collection.get(&key)?;
        Ok(
            bincode::deserialize::<(u64, HashSet<Signature>)>(&witness_signatures)
                .map_err(|_| DbError::DeserializeError)?,
        )
    }

    pub fn get_all_witness_signatures(
        &self,
    ) -> Result<Vec<(DigestIdentifier, u64, HashSet<Signature>)>, DbError> {
        let iter = self.collection.iter(false, format!("{}{}", self.prefix, char::MAX));
        Ok(iter
            .map(|ws| {
                let ws_1 = bincode::deserialize::<(u64, HashSet<Signature>)>(&ws.1).unwrap();
                (DigestIdentifier::from_str(&ws.0).unwrap(), ws_1.0, ws_1.1)
            })
            .collect())
    }

    pub fn set_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let total_signatures = match self.collection.get(&key) {
            Ok(other) => {
                let other = bincode::deserialize::<(u64, HashSet<Signature>)>(&other).unwrap();
                signatures.union(&other.1).cloned().collect()
            }
            Err(DbError::EntryNotFound) => signatures,
            Err(error) => {
                return Err(error);
            }
        };
        let Ok(data) = bincode::serialize::<(u64, HashSet<Signature>)>(&(sn, total_signatures)) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_witness_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}
