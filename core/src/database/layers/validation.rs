use crate::utils::{deserialize, serialize};
use super::utils::{get_key, Element};
use crate::commons::models::validation::ValidationProof;
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::sync::Arc;

pub(crate) struct ValidationDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> ValidationDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("validation"),
            prefix: "validation".to_string(),
        }
    }

    pub fn get_validation_register(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<ValidationProof, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let validation_register = self.collection.get(&key)?;
        Ok(deserialize::<ValidationProof>(&validation_register)
            .map_err(|_| DbError::DeserializeError)?)
    }

    pub fn set_validation_register(
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
