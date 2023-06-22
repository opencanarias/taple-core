use crate::utils::{deserialize, serialize};
use super::utils::{get_by_range, get_key, Element};
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use crate::{DbError, KeyIdentifier};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) struct PreauthorizedSbujectsAndProovidersDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> PreauthorizedSbujectsAndProovidersDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("preauthorized-subjects-and-providers"),
            prefix: "preauthorized-subjects-and-providers".to_string(),
        }
    }

    pub fn get_preauthorized_subject_and_providers(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<HashSet<KeyIdentifier>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let value = self.collection.get(&key)?;
        let result = deserialize::<(DigestIdentifier, HashSet<KeyIdentifier>)>(&value)
            .map_err(|_| DbError::DeserializeError)?;
        Ok(result.1)
    }

    pub fn get_preauthorized_subjects_and_providers(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<(DigestIdentifier, HashSet<KeyIdentifier>)>, DbError> {
        let result = get_by_range(from, quantity, &self.collection, &self.prefix)?;
        let mut vec_result = vec![];
        for value in result {
            vec_result.push(
                deserialize::<(DigestIdentifier, HashSet<KeyIdentifier>)>(&value)
                    .map_err(|_| DbError::DeserializeError)?,
            );
        }
        Ok(vec_result)
    }

    pub fn set_preauthorized_subject_and_providers(
        &self,
        subject_id: &DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<(DigestIdentifier, HashSet<KeyIdentifier>)>(&(subject_id.clone(), providers)) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }
}
