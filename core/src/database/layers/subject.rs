use super::utils::{get_by_range, get_key, Element};
use crate::commons::models::state::Subject;
use crate::DbError;
use crate::{DatabaseCollection, DatabaseManager, Derivable, DigestIdentifier};
use std::sync::Arc;

pub(crate) struct SubjectDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> SubjectDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("subject"),
            prefix: "subject".to_string(),
        }
    }

    pub fn get_subject(&self, subject_id: &DigestIdentifier) -> Result<Subject, DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let subject = self.collection.get(&key)?;
        Ok(bincode::deserialize::<Subject>(&subject).map_err(|_| {
            DbError::DeserializeError
        })?)
    }

    pub fn set_subject(
        &self,
        subject_id: &DigestIdentifier,
        subject: Subject,
    ) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<Subject>(&subject) else {
            return Err(DbError::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_subject(&self, subject_id: &DigestIdentifier) -> Result<(), DbError> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }

    pub fn get_subjects(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, DbError> {
        let subjects = get_by_range(from, quantity, &self.collection, &self.prefix.clone())?;
        Ok(subjects
            .iter()
            .map(|subject| (bincode::deserialize::<Subject>(subject).unwrap()))
            .collect())
    }

    pub fn get_governances(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, DbError> {
        // TODO: Confirmar si las gobernanzas van a tener una colección propia
        let governances = get_by_range(from, quantity, &self.collection, &self.prefix.clone())?;
        Ok(governances
            .iter()
            .map(|subject| (bincode::deserialize::<Subject>(subject).unwrap()))
            .filter(|subject| subject.schema_id == "governance")
            .collect())
    }

    pub fn get_all_subjects(&self) -> Vec<Subject> {
        // let key_elements: Vec<Element> = vec![Element::S(self.prefix.clone())];
        let mut result = Vec::new();
        for (_, subject) in self.collection.iter(false, self.prefix.clone()) {
            let subject = bincode::deserialize::<Subject>(&subject).unwrap();
            result.push(subject);
        }
        result
    }
}
