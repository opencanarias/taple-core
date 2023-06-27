use super::utils::{get_by_range, get_key, Element};
use crate::signature::Signed;
use crate::utils::{deserialize, serialize};
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier, Event};
use std::sync::Arc;

pub(crate) struct EventDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> EventDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("event"),
            prefix: "event".to_string(),
        }
    }

    pub fn get_event(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Result<Signed<Event>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let event = self.collection.get(&key)?;
        Ok(deserialize::<Signed<Event>>(&event).map_err(|_| DbError::DeserializeError)?)
    }

    pub fn get_events_by_range(
        &self,
        subject_id: &DigestIdentifier,
        from: Option<i64>,
        quantity: isize,
    ) -> Result<Vec<Signed<Event>>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let from = match from {
            Some(from) => Some(from.to_string()),
            None => None,
        };
        let events_by_subject = get_by_range(from, quantity, &self.collection, &key)?;
        Ok(events_by_subject
            .iter()
            .map(|event| deserialize::<Signed<Event>>(event).unwrap())
            .collect())
    }

    pub fn set_event(
        &self,
        subject_id: &DigestIdentifier,
        event: Signed<Event>,
    ) -> Result<(), DbError> {
        let sn = event.content.event_proposal.content.sn;
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<Signed<Event>>(&event) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}
