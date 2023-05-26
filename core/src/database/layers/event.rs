use super::utils::{get_by_range, get_key, Element};
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use crate::{DbError, Event};
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

    pub fn get_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<Event, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let event = self.collection.get(&key)?;
        Ok(bincode::deserialize::<Event>(&event).map_err(|_| {
            DbError::DeserializeError
        })?)
    }

    pub fn get_events_by_range(
        &self,
        subject_id: &DigestIdentifier,
        from: Option<i64>,
        quantity: isize,
    ) -> Result<Vec<Event>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let _subject = self.collection.get(&key)?;
        let from = match from {
            Some(from) => Some(from.to_string()),
            None => None,
        };
        let events_by_subject =
            get_by_range(from, quantity, &self.collection, &self.prefix.clone())?;
        Ok(events_by_subject
            .iter()
            .map(|event| (bincode::deserialize::<Event>(event).unwrap()))
            .collect())
    }

    pub fn set_event(&self, subject_id: &DigestIdentifier, event: Event) -> Result<(), DbError> {
        let sn = event.content.event_proposal.proposal.sn;
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<Event>(&event) else {
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
