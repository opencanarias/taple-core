use super::utils::{get_by_range, get_key, Element};
use crate::commons::models::approval::{ApprovalEntity, ApprovalState};
use crate::utils::{deserialize, serialize};
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::sync::Arc;

pub(crate) struct ApprovalsDb<C: DatabaseCollection> {
    index_collection: C,
    index_by_governance_collection: C,
    collection: C,
    pending_collection: C,
    index_prefix: String,
    prefix: String,
    governance_prefix: String,
    pending_prefix: String,
}

impl<C: DatabaseCollection> ApprovalsDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            index_collection: manager.create_collection("subjindex-approval-index"),
            index_by_governance_collection: manager.create_collection("governance-approval-index"),
            collection: manager.create_collection("approvals"),
            index_prefix: "subjindex-approval-index".to_string(),
            prefix: "approvals".to_string(),
            governance_prefix: "governance-approval-index".to_string(),
            pending_prefix: "pending-approval-index".to_string(),
            pending_collection: manager.create_collection("pending-approval-index"),
        }
    }

    pub fn set_subject_approval_index(
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

    pub fn del_subject_approval_index(
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

    pub fn set_governance_approval_index(
        &self,
        governance_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.governance_prefix.clone()),
            Element::S(governance_id.to_str()),
            Element::S(request_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<DigestIdentifier>(request_id) else {
            return Err(DbError::SerializeError);
        };
        self.index_by_governance_collection.put(&key, data)
    }

    pub fn del_governance_approval_index(
        &self,
        governance_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.governance_prefix.clone()),
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
            Element::S(self.governance_prefix.clone()),
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
                Element::S(self.governance_prefix.clone()),
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

    pub fn get_approvals(
        &self,
        status: Option<ApprovalState>,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<ApprovalEntity>, DbError> {
        let mut result = Vec::new();
        let quantity_is_positive = quantity > 0;
        let mut quantity = quantity;
        let mut from = from;
        let mut continue_while: bool = true;
        if let Some(ApprovalState::Pending) = status {
            while quantity != 0 && continue_while {
                let approvals = get_by_range(
                    from.clone(),
                    quantity,
                    &self.pending_collection,
                    &self.pending_prefix,
                )?;
                if approvals.len() < quantity.abs() as usize {
                    continue_while = false;
                }
                for approval_id in approvals.iter() {
                    let approval_id = deserialize::<DigestIdentifier>(&approval_id)
                        .map_err(|_| DbError::DeserializeError)?;
                    let key_elements: Vec<Element> = vec![
                        Element::S(self.prefix.clone()),
                        Element::S(approval_id.to_str()),
                    ];
                    let key: String = get_key(key_elements)?;
                    from = Some(key.clone());
                    match self.collection.get(&key) {
                        Ok(approval) => {
                            let approval_entity = deserialize::<ApprovalEntity>(&approval)
                                .map_err(|_| DbError::DeserializeError)?;
                            if approval_entity.state != ApprovalState::Pending {
                                self.pending_collection.del(&key)?;
                                continue;
                            }
                            if quantity_is_positive {
                                quantity -= 1;
                            } else {
                                quantity += 1;
                            }
                            result.push(approval_entity);
                        }
                        Err(e) => match e {
                            DbError::EntryNotFound => {
                                self.pending_collection.del(&key)?;
                                continue;
                            }
                            _ => return Err(e),
                        },
                    }
                }
            }
        } else {
            let approvals = get_by_range(from, quantity, &self.collection, &self.prefix)?;
            for approval in approvals.iter() {
                let approval = deserialize::<ApprovalEntity>(&approval).unwrap();
                if status.is_some() {
                    if status.as_ref().unwrap() == &approval.state {
                        result.push(approval);
                    }
                } else {
                    result.push(approval);
                }
            }
        }
        return Ok(result);
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
        // We assume that we only have pending and voted status.
        let index_key_elements: Vec<Element> = vec![
            Element::S(self.pending_prefix.clone()),
            Element::S(request_id.to_str()),
        ];
        let key2 = get_key(index_key_elements)?;
        // If it is a pending status request, it is first saved in the index and then in the supper collection.
        if approval.state == ApprovalState::Pending {
            let Ok(data2) = serialize::<DigestIdentifier>(&request_id) else {
                return Err(DbError::SerializeError);
            };
            self.pending_collection.put(&key2, data2)?;
        } else if approval.state != ApprovalState::Pending {
            self.pending_collection.del(&key2)?;
        }
        self.collection.put(&key, data)?;
        Ok(())
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
