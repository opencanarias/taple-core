use super::{deserialize, serialize};
use super::utils::{get_key, Element};
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use crate::{DbError};
use std::sync::Arc;

pub(crate) struct ContractDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> ContractDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("contract"),
            prefix: "contract".to_string(),
        }
    }

    pub fn get_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
    ) -> Result<(Vec<u8>, DigestIdentifier, u64), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(governance_id.to_str()),
            Element::S(schema_id.to_string()),
        ];
        let key = get_key(key_elements)?;
        let contract = self.collection.get(&key)?;
        Ok(deserialize::<(Vec<u8>, DigestIdentifier, u64)>(&contract).map_err(|_| {
            DbError::DeserializeError
        })?)
    }

    pub fn put_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
        contract: Vec<u8>,
        hash: DigestIdentifier,
        gov_version: u64,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(governance_id.to_str()),
            Element::S(schema_id.to_string()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<(Vec<u8>, DigestIdentifier, u64)>(&(contract, hash, gov_version)) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn get_governance_contract(&self) -> Result<Vec<u8>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S("governance".to_string()),
        ];
        let key = get_key(key_elements)?;
        let contract = self.collection.get(&key)?;
        let contract = deserialize::<(Vec<u8>, DigestIdentifier, u64)>(&contract).map_err(|_| {
            DbError::DeserializeError
        })?;
        Ok(contract.0)
    }

    pub fn put_governance_contract(&self, contract: Vec<u8>) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S("governance".to_string()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<(Vec<u8>, DigestIdentifier, u64)>(&(contract, DigestIdentifier::default(), 0)) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }
}