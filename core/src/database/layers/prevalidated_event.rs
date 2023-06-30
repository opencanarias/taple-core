use crate::signature::Signed;
use crate::utils::{deserialize, serialize};
use super::utils::{get_key, Element};
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier, Event};
use crate::{DbError};
use std::sync::Arc;

pub(crate) struct PrevalidatedEventDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> PrevalidatedEventDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("prevalidated-event"),
            prefix: "prevalidated-event".to_string(),
        }
    }

    pub fn get_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<Signed<Event>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let prevalidated_event = self.collection.get(&key)?;
        Ok(deserialize::<Signed<Event>>(&prevalidated_event).map_err(|_| {
            DbError::DeserializeError
        })?)
    }

    pub fn set_prevalidated_event(
        &self,
        subject_id: &DigestIdentifier,
        event: Signed<Event>,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<Signed<Event>>(&event) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}