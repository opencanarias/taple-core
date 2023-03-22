use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::commons::models::state::{LedgerState, Subject};
use crate::event_content::EventContent;
use crate::event_request::EventRequest;
use crate::identifier::{Derivable, DigestIdentifier, KeyIdentifier};
use crate::signature::Signature;
use crate::Event;

use super::error::Error;
use super::{DatabaseCollection, DatabaseManager};

const SIGNATURE_TABLE: &str = "signature";
const SUBJECT_TABLE: &str = "subject";
const EVENT_TABLE: &str = "event";
const REQUEST_TABLE: &str = "request";
const ID_TABLE: &str = "controller-id";
const NOTARY_TABLE: &str = "notary";
const CONTRACT_TABLE: &str = "contract";

pub struct DB<M: DatabaseManager> {
    _manager: Arc<M>,
    signature_db: Box<dyn DatabaseCollection<InnerDataType = HashSet<Signature>>>,
    subject_db: Box<dyn DatabaseCollection<InnerDataType = Subject>>,
    event_db: Box<dyn DatabaseCollection<InnerDataType = Event>>,
    request_db: Box<dyn DatabaseCollection<InnerDataType = EventRequest>>,
    id_db: Box<dyn DatabaseCollection<InnerDataType = String>>,
    notary_db: Box<dyn DatabaseCollection<InnerDataType = (DigestIdentifier, u64)>>,
    contract_db: Box<dyn DatabaseCollection<InnerDataType = (Vec<u8>, Vec<u8>)>>,
}

impl<M: DatabaseManager> DB<M> {
    pub fn new(manager: Arc<M>) -> Self {
        let signature_db = manager.create_collection(SIGNATURE_TABLE);
        let subject_db = manager.create_collection(SUBJECT_TABLE);
        let event_db = manager.create_collection(EVENT_TABLE);
        let request_db = manager.create_collection(REQUEST_TABLE);
        let id_db = manager.create_collection(ID_TABLE);
        let notary_db = manager.create_collection(NOTARY_TABLE);
        let contract_db = manager.create_collection(CONTRACT_TABLE);
        Self {
            _manager: manager,
            signature_db,
            subject_db,
            event_db,
            request_db,
            id_db,
            notary_db,
            contract_db,
        }
    }

    pub fn get_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<Event, Error> {
        let id = subject_id.to_str();
        let events_by_subject = self.event_db.partition(&id);
        events_by_subject.get(&sn.to_string())
    }

    fn get_by_range<'a, V: Serialize + DeserializeOwned + Sync + Send>(
        &'a self,
        from: Option<String>,
        quantity: isize,
        partition: &Box<dyn DatabaseCollection<InnerDataType = V> + 'a>,
    ) -> Result<Vec<V>, Error> {
        fn convert<'a, V: Serialize + DeserializeOwned>(
            iter: impl Iterator<Item = (String, V)> + 'a,
        ) -> Box<dyn Iterator<Item = (String, V)> + 'a> {
            Box::new(iter)
        }
        let (mut iter, quantity) = match from {
            Some(key) => {
                // Find the key
                let iter = if quantity >= 0 {
                    partition.iter()
                } else {
                    partition.rev_iter()
                };
                let mut iter = iter.peekable();
                loop {
                    let Some((current_key, _)) = iter.peek() else {
                        return Err(Error::EntryNotFound);
                    };
                    if current_key == &key {
                        break;
                    }
                    iter.next();
                }
                (convert(iter), quantity.abs())
            }
            None => {
                if quantity < 0 {
                    (partition.rev_iter(), quantity.abs())
                } else {
                    (partition.iter(), quantity)
                }
            }
        };
        let mut result = Vec::new();
        let mut counter = 0;
        while counter < quantity {
            let Some((_, event)) = iter.next() else {
              break;
            };
            result.push(event);
            counter += 1;
        }
        Ok(result)
    }

    pub fn get_events_by_range(
        &self,
        subject_id: &DigestIdentifier,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Event>, Error> {
        let id = subject_id.to_str();
        let events_by_subject = self.event_db.partition(&id);
        self.get_by_range(from, quantity, &events_by_subject)
    }

    pub fn set_event(&self, subject_id: &DigestIdentifier, event: Event) -> Result<(), Error> {
        // TODO: DETERMINAR SI DEVOLVER RESULT
        let id = subject_id.to_str();
        let events_by_subject = self.event_db.partition(&id);
        let sn = event.event_content.sn.to_string();
        events_by_subject.put(&sn, event)
    }

    pub fn get_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Result<HashSet<Signature>, Error> {
        let id = subject_id.to_str();
        let events_by_subject = self.signature_db.partition(&id);
        events_by_subject.get(&sn.to_string())
    }

    pub fn set_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), Error> {
        let id = subject_id.to_str();
        let signatures_by_subject = self.signature_db.partition(&id);
        let sn = sn.to_string();
        let total_signatures = match signatures_by_subject.get(&sn.to_string()) {
            Ok(other) => signatures.union(&other).cloned().collect(),
            Err(Error::EntryNotFound) => signatures,
            Err(error) => {
                // logError!("Error detected in database get_event operation: {}", error);
                return Err(error);
            }
        };
        signatures_by_subject.put(&sn.to_string(), total_signatures)
    }

    pub fn get_subject(&self, subject_id: &DigestIdentifier) -> Result<Subject, Error> {
        self.subject_db.get(&subject_id.to_str())
    }

    pub fn set_subject(
        &self,
        subject_id: &DigestIdentifier,
        subject: Subject,
    ) -> Result<(), Error> {
        let id = subject_id.to_str();
        self.subject_db.put(&id, subject)
    }

    pub fn set_negociating_true(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let mut subject = self.get_subject(subject_id)?;
        subject.ledger_state.negociating_next = true;
        // Persist the change
        self.set_subject(&subject_id, subject)
    }

    pub fn apply_event_sourcing(&self, event_content: EventContent) -> Result<(), Error> {
        // TODO: Consultar sobre si este método debería existir
        let subject_id = event_content.subject_id.clone();
        let mut subject = self.get_subject(&subject_id)?;
        subject
            .apply(event_content.clone())
            .map_err(|_| Error::SubjectApplyFailed)?;
        // Persist the change
        self.set_subject(&subject_id, subject)?;
        let id = subject_id.to_str();
        let signatures_by_subject = self.signature_db.partition(&id);
        match signatures_by_subject.del(&(event_content.sn - 1).to_string()) {
            Ok(_) | Err(Error::EntryNotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub fn get_all_heads(&self) -> Result<HashMap<DigestIdentifier, LedgerState>, Error> {
        let mut result = HashMap::new();
        for (key, subject) in self.subject_db.iter() {
            let subject_id =
                DigestIdentifier::from_str(&key).map_err(|_| Error::NoDigestIdentifier)?;
            result.insert(subject_id, subject.ledger_state);
        }
        Ok(result)
    }

    pub fn get_all_subjects(&self) -> Vec<Subject> {
        let mut result = Vec::new();
        for (_, subject) in self.subject_db.iter() {
            result.push(subject);
        }
        result
    }

    pub fn get_subjects(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, Error> {
        self.get_by_range(from, quantity, &self.subject_db)
    }

    pub fn get_governances(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, Error> {
        // TODO: Confirmar si las gobernanzas van a tener una colección propia
        self.get_by_range(from, quantity, &self.subject_db)
    }

    pub fn get_all_request(&self) -> Vec<EventRequest> {
        let mut result = Vec::new();
        for (_, request) in self.request_db.iter() {
            result.push(request);
        }
        result
    }

    pub fn get_request(
        &self,
        subject_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Result<EventRequest, Error> {
        let id = subject_id.to_str();
        let requests_by_subject = self.request_db.partition(&id);
        requests_by_subject.get(&request_id.to_str())
    }

    pub fn set_request(
        &self,
        subject_id: &DigestIdentifier,
        request: EventRequest,
    ) -> Result<(), Error> {
        let id = subject_id.to_str();
        let requests_by_subject = self.request_db.partition(&id);
        let req_id = request.signature.content.event_content_hash.to_str();
        requests_by_subject.put(&req_id, request)
    }

    pub fn del_request(
        &self,
        subject_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Result<(), Error> {
        let id = subject_id.to_str();
        let requests_by_subject = self.request_db.partition(&id);
        requests_by_subject.del(&request_id.to_str())
    }

    pub fn get_controller_id(&self) -> Result<String, Error> {
        self.id_db.get("")
    }

    pub fn set_controller_id(&self, controller_id: String) -> Result<(), Error> {
        self.id_db.put("", controller_id)
    }

    pub fn set_notary_register(
        &self,
        owner: &KeyIdentifier,
        subject_id: &DigestIdentifier,
        event_hash: DigestIdentifier,
        sn: u64,
    ) -> Result<(), Error> {
        let owner_id = owner.to_str();
        let subject_id = subject_id.to_str();
        let notary_partition = self.notary_db.partition(&owner_id);
        if let Err(error) = notary_partition.put(&subject_id, (event_hash, sn)) {
            return Err(error);
        }
        Ok(())
    }
    pub fn get_notary_register(
        &self,
        owner: &KeyIdentifier,
        subject_id: &DigestIdentifier,
    ) -> Result<(DigestIdentifier, u64), Error> {
        let owner_id = owner.to_str();
        let subject_id = subject_id.to_str();
        let notary_partition = self.notary_db.partition(&owner_id);
        notary_partition.get(&subject_id)
    }

    // Contracts Section
    pub fn get_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
    ) -> Result<(Vec<u8>, Vec<u8>), Error> {
        let id = governance_id.to_str();
        let schemas_by_governances = self.contract_db.partition(&id);
        schemas_by_governances.get(schema_id)
    }

    pub fn put_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
        contract: Vec<u8>,
        hash: Vec<u8>,
    ) -> Result<(), Error> {
        let id = governance_id.to_str();
        let schemas_by_governances = self.contract_db.partition(&id);
        schemas_by_governances.put(schema_id, (contract, hash))
    }
}
