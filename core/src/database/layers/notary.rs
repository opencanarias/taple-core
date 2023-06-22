use super::{deserialize, serialize};
use super::utils::{get_key, Element};
use crate::commons::models::event::ValidationProof;
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
        subject_id: &DigestIdentifier,
    ) -> Result<ValidationProof, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let notary_register = self.collection.get(&key)?;
        Ok(deserialize::<ValidationProof>(&notary_register)
            .map_err(|_| DbError::DeserializeError)?)
    }

    pub fn set_notary_register(
        &self,
        subject_id: &DigestIdentifier,
        validation_proof: &ValidationProof,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<ValidationProof>(validation_proof) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }
}
