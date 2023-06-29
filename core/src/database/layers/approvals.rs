use super::utils::{get_key, Element};
use crate::commons::models::approval::{ApprovalEntity, ApprovalState};
use crate::utils::{deserialize, serialize};
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::sync::Arc;

pub(crate) struct ApprovalsDb<C: DatabaseCollection> {
    index_collection: C,
    index_by_governance_collection: C,
    collection: C,
    index_prefix: String,
    prefix: String,
}

impl<C: DatabaseCollection> ApprovalsDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            index_collection: manager.create_collection("subject-approval-index"),
            index_by_governance_collection: manager.create_collection("governance-approval-index"),
            collection: manager.create_collection("approvals"),
            index_prefix: "subject-approval-index".to_string(),
            prefix: "approvals".to_string(),
        }
    }

    pub fn set_subject_aproval_index(
        &self,
        subject_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.index_prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<DigestIdentifier>(request_id) else {
          return Err(DbError::SerializeError);
        };
        self.index_collection.put(&key, data)
    }

    pub fn del_subject_aproval_index(
        &self,
        subject_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.index_prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.index_collection.del(&key)
    }

    pub fn set_governance_aproval_index(
        &self,
        governance_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.index_prefix.clone()),
            Element::S(governance_id.to_str()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<DigestIdentifier>(request_id) else {
          return Err(DbError::SerializeError);
        };
        self.index_by_governance_collection.put(&key, data)
    }

    pub fn del_governance_aproval_index(
        &self,
        governance_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.index_prefix.clone()),
            Element::S(governance_id.to_str()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.index_by_governance_collection.del(&key)
    }

    pub fn get_approvals_by_governance(
        &self,
        governance_id: &DigestIdentifier,
    ) -> Result<Vec<DigestIdentifier>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.index_prefix.clone()),
            Element::S(governance_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let iter = self.index_by_governance_collection.iter(false, key);
        let mut result = Vec::new();
        let mut to_delete = Vec::new();
        for (_, data) in iter {
            let Ok(request_id) = deserialize::<DigestIdentifier>(&data) else {
                return Err(DbError::SerializeError);
            };
            // Comprobamos si existe en la colección base
            match self.get_approval(&request_id) {
                Ok(data) => match data.state {
                    ApprovalState::Pending => result.push(request_id),
                    _ => to_delete.push(request_id),
                },
                Err(DbError::EntryNotFound) => to_delete.push(request_id),
                Err(error) => return Err(error),
            }
        }
        for request_id in to_delete {
            let key_elements: Vec<Element> = vec![
                Element::S(self.index_prefix.clone()),
                Element::S(governance_id.to_str()),
                Element::S(request_id.to_str()),
            ];
            let key = get_key(key_elements)?;
            self.index_by_governance_collection.del(&key)?;
        }
        Ok(result)
    }

    pub fn get_approvals_by_subject(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<Vec<DigestIdentifier>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.index_prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let iter = self.index_collection.iter(false, key);
        let mut result = Vec::new();
        let mut to_delete = Vec::new();
        for (_, data) in iter {
            let Ok(request_id) = deserialize::<DigestIdentifier>(&data) else {
                return Err(DbError::SerializeError);
            };
            // Comprobamos si existe en la colección base
            match self.get_approval(&request_id) {
                Ok(data) => match data.state {
                    ApprovalState::Pending => result.push(request_id),
                    _ => to_delete.push(request_id),
                },
                Err(DbError::EntryNotFound) => to_delete.push(request_id),
                Err(error) => return Err(error),
            }
        }
        for request_id in to_delete {
            let key_elements: Vec<Element> = vec![
                Element::S(self.index_prefix.clone()),
                Element::S(subject_id.to_str()),
                Element::S(request_id.to_str()),
            ];
            let key = get_key(key_elements)?;
            self.index_collection.del(&key)?;
        }
        Ok(result)
    }

    pub fn get_approval(&self, request_id: &DigestIdentifier) -> Result<ApprovalEntity, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let approval = self.collection.get(&key)?;
        Ok(deserialize::<ApprovalEntity>(&approval).map_err(|_| DbError::DeserializeError)?)
    }

    pub fn get_approvals(&self, status: Option<String>) -> Result<Vec<ApprovalEntity>, DbError> {
        let mut result = Vec::new();
        match status {
            Some(value) => {
                let real_status = match value.as_str() {
                    "Pending" => ApprovalState::Pending,
                    "Responded" => ApprovalState::Responded,
                    "Obsolete" => ApprovalState::Obsolete,
                    _ => return Err(DbError::NonExistentStatus),
                };
                for (_, approval) in self
                    .collection
                    .iter(false, format!("{}{}", self.prefix, char::MAX))
                {
                    let approval = deserialize::<ApprovalEntity>(&approval).unwrap();
                    if approval.state == real_status {
                        result.push(approval);
                    }
                }
                return Ok(result);
            }
            None => {
                for (_, approval) in self
                    .collection
                    .iter(false, format!("{}{}", self.prefix, char::MAX))
                {
                    let approval = deserialize::<ApprovalEntity>(&approval).unwrap();
                    result.push(approval);
                }
                return Ok(result);
            }
        }
    }

    pub fn set_approval(
        &self,
        request_id: &DigestIdentifier,
        approval: ApprovalEntity,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<ApprovalEntity>(&approval) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_approval(&self, request_id: &DigestIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}
