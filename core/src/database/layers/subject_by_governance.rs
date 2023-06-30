use crate::utils::{deserialize, serialize};
use super::utils::{get_key, Element, get_by_range_governances};
use crate::commons::models::state::Subject;
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::sync::Arc;

pub(crate) struct SubjectByGovernanceDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> SubjectByGovernanceDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: &Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("governance-index"),
            prefix: "governance-index".to_string(),
        }
    }

    pub fn set_governance_index(
        &self,
        subject_id: &DigestIdentifier,
        governance_id: &DigestIdentifier,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(governance_id.to_str()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = serialize::<DigestIdentifier>(subject_id) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn get_subjects_by_governance(
        &self,
        governance_id: &DigestIdentifier,
    ) -> Result<Vec<DigestIdentifier>, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(governance_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let mut result = Vec::new();
        for (_, data) in self.collection.iter(false, format!("{}{}", key, char::MAX)) {
            let request = deserialize::<DigestIdentifier>(&data)
                .map_err(|_| DbError::DeserializeError)?;
            result.push(request);
        }
        Ok(result)
    }

    pub fn get_governances(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, DbError> {
        let governances = match from {
            Some(_) => {
                get_by_range_governances(
                    from,
                    quantity,
                    &self.collection,
                    &format!("{}{}", &self.prefix.clone(), char::MAX),
                )?
            }
            None => {
                get_by_range_governances(
                    from,
                    quantity,
                    &self.collection,
                    &format!("{}{}{}", &self.prefix.clone(), char::MAX, char::MAX),
                )?
            }
        };  
        Ok(self.return_subjects(governances)?)
    }

    pub fn get_governance_subjects(
        &self,
        governance_id: &DigestIdentifier,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, DbError> {
        let subjects = get_by_range_governances(
            from,
            quantity,
            &self.collection,
            &format!(
                "{}{}{}",
                &self.prefix.clone(),
                char::MAX,
                governance_id.to_str()
            ),
        )?;
        Ok(self.return_subjects(subjects)?)
    }

    fn return_subjects(&self, values: Vec<Vec<u8>>) -> Result<Vec<Subject>, DbError> {
        let mut subjects = vec![];
        let subject_prefix = "subject".to_string();
        for value in values {
            let subject_id = deserialize::<DigestIdentifier>(&value)
                .map_err(|_| DbError::DeserializeError)?;
            let key = format!("{}{}{}", subject_prefix, char::MAX, subject_id.to_str());
            let subject = deserialize::<Subject>(&self.collection.get(&key)?)
                .map_err(|_| DbError::DeserializeError)?;
            subjects.push(subject);
        }
        Ok(subjects)
    }
}
