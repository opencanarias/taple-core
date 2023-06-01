use super::utils::{get_key, Element};
use crate::commons::models::event::ValidationProof;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use crate::{DbError, Event};
use std::sync::Arc;

pub(crate) struct LceValidationProofs<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> LceValidationProofs<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("lce-validation-proofs"),
            prefix: "lce-validation-proofs".to_string(),
        }
    }

    pub fn get_lce_validation_proof(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<ValidationProof, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let lce_validation_proof = self.collection.get(&key)?;
        Ok(
            bincode::deserialize::<ValidationProof>(&lce_validation_proof)
                .map_err(|_| DbError::DeserializeError)?,
        )
    }

    pub fn set_lce_validation_proof(
        &self,
        subject_id: &DigestIdentifier,
        lce_validation_proof: ValidationProof,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<ValidationProof>(&lce_validation_proof) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_lce_validation_proof(&self, subject_id: &DigestIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}
