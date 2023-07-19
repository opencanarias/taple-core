use super::utils::{get_key, Element};
use crate::commons::models::request::TapleRequest;
use crate::utils::{deserialize, serialize};
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::sync::Arc;

pub(crate) struct RequestDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> RequestDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("taple-request"),
            prefix: "taple-request".to_string(),
        }
    }

    pub fn get_request(&self, request_id: &DigestIdentifier) -> Result<TapleRequest, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let request = self.collection.get(&key)?;
        Ok(deserialize::<TapleRequest>(&request).map_err(|_| DbError::DeserializeError)?)
    }

    pub fn get_all_request(&self) -> Vec<TapleRequest> {
        let mut result = Vec::new();
        for (_, request) in self
            .collection
            .iter(false, format!("{}{}", self.prefix, char::MAX))
        {
            let request = deserialize::<TapleRequest>(&request).unwrap();
            result.push(request);
        }
        result
    }

    pub fn set_request(
        &self,
        request_id: &DigestIdentifier,
        request: &TapleRequest,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<TapleRequest>(request) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_request(&self, request_id: &DigestIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}
