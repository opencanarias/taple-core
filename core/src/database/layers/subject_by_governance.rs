use super::utils::{get_key, Element};
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::sync::Arc;

pub(crate) struct SubjectByGovernanceDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> SubjectByGovernanceDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("governance-index"),
            prefix: "governance-index".to_string(),
        }
    }
    
    pub fn set_governance_index(
        &self,
        subject_id: &DigestIdentifier,
        gobernance_id: &DigestIdentifier,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(gobernance_id.to_str()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<DigestIdentifier>(subject_id) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }
    
    pub fn get_subjects_by_governance(
        &self,
        gobernance_id: &DigestIdentifier,
    ) -> Result<Vec<DigestIdentifier>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(gobernance_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let mut result = Vec::new();
        for (_, data) in self.collection.iter(false, format!("{}{}", key, char::MAX)) {
            let request = bincode::deserialize::<DigestIdentifier>(&data).map_err(|_| {
                DbError::DeserializeError
            })?;
            result.push(request);
        }
        Ok(result)
    }
}