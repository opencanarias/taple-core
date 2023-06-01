use super::utils::{get_key, Element};
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier, KeyIdentifier};
use std::sync::Arc;

pub(crate) struct NotaryDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> NotaryDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("notary"),
            prefix: "notary".to_string(),
        }
    }

    pub fn get_notary_register(
        &self,
        owner: &KeyIdentifier,
        subject_id: &DigestIdentifier,
    ) -> Result<(DigestIdentifier, u64), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(owner.to_str()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let notary_register = self.collection.get(&key)?;
        Ok(bincode::deserialize::<(DigestIdentifier, u64)>(&notary_register).map_err(|_| {
            DbError::DeserializeError
        })?)
    }

    pub fn set_notary_register(
        &self,
        owner: &KeyIdentifier,
        subject_id: &DigestIdentifier,
        event_hash: DigestIdentifier,
        sn: u64,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(owner.to_str()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<(DigestIdentifier, u64)>(&(event_hash, sn)) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }
}