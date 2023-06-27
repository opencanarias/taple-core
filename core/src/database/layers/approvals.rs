use crate::utils::{deserialize, serialize};
use super::utils::{get_by_range, get_key, Element};
use crate::commons::models::approval::ApprovalState;
use crate::{DbError, ApprovalPetitionData};
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::sync::Arc;

pub(crate) struct ApprovalsDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> ApprovalsDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("approvals"),
            prefix: "approvals".to_string(),
        }
    }

    pub fn get_approval(
        &self,
        request_id: &DigestIdentifier,
    ) -> Result<(ApprovalPetitionData, ApprovalState), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let approval = self.collection.get(&key)?;
        Ok(
            deserialize::<(ApprovalPetitionData, ApprovalState)>(&approval)
                .map_err(|_| DbError::DeserializeError)?,
        )
    }

    pub fn get_approvals(
        &self,
        status: Option<String>,
    ) -> Result<Vec<ApprovalPetitionData>, DbError> {
        let mut result = Vec::new();
        match status {
            Some(value) => {
                let real_status = match value.as_str() {
                    "Pending" => ApprovalState::Pending,
                    "Responded" => ApprovalState::Responded,
                    "Obsolete" => ApprovalState::Obsolete,
                    _ => return Err(DbError::NonExistentStatus),
                };
                for (_, approval) in self.collection.iter(false, format!("{}{}", self.prefix, char::MAX)) {
                    let approval = deserialize::<(ApprovalPetitionData, ApprovalState)>(&approval).unwrap();
                    if approval.1 == real_status {
                        result.push(approval.0);
                    }
                }
                return Ok(result);
            }
            None => {
                for (_, approval) in self.collection.iter(false, format!("{}{}", self.prefix, char::MAX)) {
                    let approval = deserialize::<(ApprovalPetitionData, ApprovalState)>(&approval).unwrap();
                    result.push(approval.0);
                }
                return Ok(result);
            }
        }
    }

    pub fn set_approval(
        &self,
        request_id: &DigestIdentifier,
        approval: (ApprovalPetitionData, ApprovalState),
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<(ApprovalPetitionData, ApprovalState)>(&approval) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_approval(
        &self,
        request_id: &DigestIdentifier
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}