use std::collections::HashSet;
use std::sync::Arc;

use crate::commons::models::notary::NotaryEventResponse;
use crate::commons::models::state::Subject;
use crate::event_request::EventRequest;
use crate::identifier::{DigestIdentifier, KeyIdentifier};
use crate::signature::Signature;
use crate::Event;

use super::error::Error;
use super::{
    layers::{
        contract::ContractDb, controller_id::ControllerIdDb, event::EventDb, notary::NotaryDb,
        notary_signatures::NotarySignaturesDb, prevalidated_event::PrevalidatedEventDb,
        request::RequestDb, signature::SignatureDb, subject::SubjectDb,
        subject_by_governance::SubjectByGovernanceDb, witness_signatures::WitnessSignaturesDb,
    },
    DatabaseCollection, DatabaseManager,
};

pub struct DB<C: DatabaseCollection> {
    signature_db: SignatureDb<C>,
    subject_db: SubjectDb<C>,
    event_db: EventDb<C>,
    prevalidated_event_db: PrevalidatedEventDb<C>,
    request_db: RequestDb<C>,
    controller_id_db: ControllerIdDb<C>,
    notary_db: NotaryDb<C>,
    contract_db: ContractDb<C>,
    notary_signatures_db: NotarySignaturesDb<C>,
    witness_signatures_db: WitnessSignaturesDb<C>,
    subject_by_governance_db: SubjectByGovernanceDb<C>,
}

impl<C: DatabaseCollection> DB<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        let signature_db = SignatureDb::new(manager.clone());
        let subject_db = SubjectDb::new(manager.clone());
        let event_db = EventDb::new(manager.clone());
        let prevalidated_event_db = PrevalidatedEventDb::new(manager.clone());
        let request_db = RequestDb::new(manager.clone());
        let controller_id_db = ControllerIdDb::new(manager.clone());
        let notary_db = NotaryDb::new(manager.clone());
        let contract_db = ContractDb::new(manager.clone());
        let notary_signatures_db = NotarySignaturesDb::new(manager.clone());
        let witness_signatures_db = WitnessSignaturesDb::new(manager.clone());
        let subject_by_governance_db = SubjectByGovernanceDb::new(manager);
        Self {
            signature_db,
            subject_db,
            event_db,
            prevalidated_event_db,
            request_db,
            controller_id_db,
            notary_db,
            contract_db,
            notary_signatures_db,
            witness_signatures_db,
            subject_by_governance_db,
        }
    }

    pub fn get_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Result<HashSet<Signature>, Error> {
        self.signature_db.get_signatures(subject_id, sn)
    }

    pub fn set_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), Error> {
        self.signature_db.set_signatures(subject_id, sn, signatures)
    }

    pub fn del_signatures(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), Error> {
        self.signature_db.del_signatures(subject_id, sn)
    }

    pub fn get_subject(&self, subject_id: &DigestIdentifier) -> Result<Subject, Error> {
        self.subject_db.get_subject(subject_id)
    }

    pub fn set_subject(
        &self,
        subject_id: &DigestIdentifier,
        subject: Subject,
    ) -> Result<(), Error> {
        self.subject_db.set_subject(subject_id, subject)
    }

    pub fn get_subjects(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, Error> {
        self.subject_db.get_subjects(from, quantity)
    }

    pub fn del_subject(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.subject_db.del_subject(subject_id)
    }

    pub fn get_governances(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, Error> {
        self.subject_db.get_governances(from, quantity)
    }

    pub fn get_all_subjects(&self) -> Vec<Subject> {
        self.subject_db.get_all_subjects()
    }

    pub fn get_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<Event, Error> {
        self.event_db.get_event(subject_id, sn)
    }

    pub fn get_events_by_range(
        &self,
        subject_id: &DigestIdentifier,
        from: Option<i64>,
        quantity: isize,
    ) -> Result<Vec<Event>, Error> {
        self.event_db
            .get_events_by_range(subject_id, from, quantity)
    }

    pub fn set_event(&self, subject_id: &DigestIdentifier, event: Event) -> Result<(), Error> {
        self.event_db.set_event(subject_id, event)
    }

    pub fn del_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), Error> {
        self.event_db.del_event(subject_id, sn)
    }

    pub fn get_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<Event, Error> {
        self.prevalidated_event_db
            .get_prevalidated_event(subject_id)
    }

    pub fn set_prevalidated_event(
        &self,
        subject_id: &DigestIdentifier,
        event: Event,
    ) -> Result<(), Error> {
        self.prevalidated_event_db
            .set_prevalidated_event(subject_id, event)
    }

    pub fn del_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.prevalidated_event_db
            .del_prevalidated_event(subject_id)
    }

    pub fn get_request(&self, subject_id: &DigestIdentifier) -> Result<EventRequest, Error> {
        self.request_db.get_request(subject_id)
    }

    pub fn get_all_request(&self) -> Vec<EventRequest> {
        self.request_db.get_all_request()
    }

    pub fn set_request(
        &self,
        subject_id: &DigestIdentifier,
        request: EventRequest,
    ) -> Result<(), Error> {
        self.request_db.set_request(subject_id, request)
    }

    pub fn del_request(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.request_db.del_request(subject_id)
    }

    pub fn get_controller_id(&self) -> Result<String, Error> {
        self.controller_id_db.get_controller_id()
    }

    pub fn set_controller_id(&self, controller_id: String) -> Result<(), Error> {
        self.controller_id_db.set_controller_id(controller_id)
    }

    pub fn get_notary_register(
        &self,
        owner: &KeyIdentifier,
        subject_id: &DigestIdentifier,
    ) -> Result<(DigestIdentifier, u64), Error> {
        self.notary_db.get_notary_register(owner, subject_id)
    }

    pub fn set_notary_register(
        &self,
        owner: &KeyIdentifier,
        subject_id: &DigestIdentifier,
        event_hash: DigestIdentifier,
        sn: u64,
    ) -> Result<(), Error> {
        self.notary_db
            .set_notary_register(owner, subject_id, event_hash, sn)
    }

    pub fn get_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
    ) -> Result<(Vec<u8>, DigestIdentifier, u64), Error> {
        self.contract_db.get_contract(governance_id, schema_id)
    }

    pub fn put_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
        contract: Vec<u8>,
        hash: DigestIdentifier,
        gov_version: u64,
    ) -> Result<(), Error> {
        self.contract_db
            .put_contract(governance_id, schema_id, contract, hash, gov_version)
    }

    pub fn get_governance_contract(&self) -> Result<Vec<u8>, Error> {
        self.contract_db.get_governance_contract()
    }

    pub fn put_governance_contract(&self, contract: Vec<u8>) -> Result<(), Error> {
        self.contract_db.put_governance_contract(contract)
    }

    pub fn get_notary_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Result<(u64, HashSet<NotaryEventResponse>), Error> {
        self.notary_signatures_db
            .get_notary_signatures(subject_id, sn)
    }

    pub fn set_notary_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<NotaryEventResponse>,
    ) -> Result<(), Error> {
        self.notary_signatures_db
            .set_notary_signatures(subject_id, sn, signatures)
    }

    pub fn delete_notary_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.notary_signatures_db
            .delete_notary_signatures(subject_id)
    }

    pub fn get_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<(u64, HashSet<Signature>), Error> {
        self.witness_signatures_db
            .get_witness_signatures(subject_id)
    }

    pub fn get_all_witness_signatures(
        &self,
    ) -> Result<Vec<(DigestIdentifier, u64, HashSet<Signature>)>, Error> {
        self.witness_signatures_db.get_all_witness_signatures()
    }

    pub fn set_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), Error> {
        self.witness_signatures_db
            .set_witness_signatures(subject_id, sn, signatures)
    }

    pub fn del_witness_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.witness_signatures_db
            .del_witness_signatures(subject_id)
    }

    pub fn set_governance_index(
        &self,
        subject_id: &DigestIdentifier,
        gobernance_id: &DigestIdentifier,
    ) -> Result<(), Error> {
        self.subject_by_governance_db
            .set_governance_index(subject_id, gobernance_id)
    }

    pub fn get_subjects_by_governance(
        &self,
        gobernance_id: &DigestIdentifier,
    ) -> Result<Vec<DigestIdentifier>, Error> {
        self.subject_by_governance_db
            .get_subjects_by_governance(gobernance_id)
    }
}

/*
const MAX_U64: usize = 17; // Max size u64

enum Element {
    N(u64),
    S(String),
}

fn get_u64_as_hexadecimal(value: u64) -> String {
    format!("{:0width$}", format!("{:016x}", value), width = MAX_U64)
}

fn get_key(key_elements: Vec<Element>) -> Result<String, Error> {
    if key_elements.len() > 0 {
        let mut key: String = String::from("");
        for i in 0..(key_elements.len() - 1) {
            key.push_str(&{
                match &key_elements[i] {
                    Element::N(n) => get_u64_as_hexadecimal(*n),
                    Element::S(s) => s.to_string(),
                }
            });
            key.push_str(&char::MAX.to_string());
        }
        key.push_str(&{
            match &key_elements[key_elements.len() - 1] {
                Element::N(n) => get_u64_as_hexadecimal(*n),
                Element::S(s) => s.to_string(),
            }
        });
        Ok(format!("{}", key))
    } else {
        Err(Error::KeyElementsError)
    }
}

fn get_by_range<C: DatabaseCollection>(
    from: Option<String>,
    quantity: isize,
    collection: &C,
    prefix: &str,
) -> Result<Vec<Vec<u8>>, Error> {
    fn convert<'a>(
        iter: impl Iterator<Item = (String, Vec<u8>)> + 'a,
    ) -> Box<dyn Iterator<Item = (String, Vec<u8>)> + 'a> {
        Box::new(iter)
    }

    let (mut iter, quantity) = match from {
        Some(key) => {
            // Get true key
            let key_elements: Vec<Element> = vec![Element::S(prefix.to_string()), Element::S(key)];
            let key = get_key(key_elements)?;
            // Find the key
            let iter = if quantity >= 0 {
                collection.iter(false, prefix.to_string())
            } else {
                collection.iter(true, prefix.to_string())
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
            iter.next(); // Exclusive From
            (convert(iter), quantity.abs())
        }
        None => {
            if quantity >= 0 {
                (collection.iter(false, prefix.to_string()), quantity)
            } else {
                (collection.iter(true, prefix.to_string()), quantity.abs())
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

struct SignatureDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> SignatureDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("signature"),
            prefix: "signature".to_string(),
        }
    }

    pub fn get_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Result<HashSet<Signature>, Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let signatures = self.collection.get(&key)?;
        Ok(bincode::deserialize::<HashSet<Signature>>(&signatures).unwrap())
    }

    pub fn set_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let total_signatures = match self.collection.get(&key) {
            Ok(other) => {
                let other = bincode::deserialize::<HashSet<Signature>>(&other).unwrap();
                signatures.union(&other).cloned().collect()
            }
            Err(Error::EntryNotFound) => signatures,
            Err(error) => {
                return Err(error);
            },
            Err(error) => {
                // logError!("Error detected in database get_event operation: {}", error);
                return Err(error);
            }
        };
        self.notary_signatures_db.put(&id, (sn, total_signatures))
    }
}

struct SubjectDb<C: DatabaseCollection> {
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

    pub fn get_subject(&self, subject_id: &DigestIdentifier) -> Result<Subject, Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let subject = self.collection.get(&key)?;
        Ok(bincode::deserialize::<Subject>(&subject).unwrap())
    }

    pub fn set_governance_index(
        &self,
        subject_id: &DigestIdentifier,
        gobernance_id: &DigestIdentifier,
    ) -> Result<(), Error> {
        let intermediate_partition = self.subjects_by_governance.partition("xxxxxxxx");
        let subjects_by_governance = intermediate_partition.partition(&gobernance_id.to_str());
        subjects_by_governance.put(&subject_id.to_str(), subject_id.clone())
    }

    pub fn get_subjects_by_governance(
        &self,
        gobernance_id: &DigestIdentifier,
    ) -> Result<Vec<DigestIdentifier>, Error> {
        let intermediate_partition = self.subjects_by_governance.partition("xxxxxxxx");
        let subjects_by_governance = intermediate_partition.partition(&gobernance_id.to_str());
        let iter = subjects_by_governance.iter();
        Ok(iter.map(|(_, id)| id).collect())
    }

    pub fn set_subject(
        &self,
        subject_id: &DigestIdentifier,
        subject: Subject,
    ) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<Subject>(&subject) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_subject(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
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
    ) -> Result<Vec<Subject>, Error> {
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
    ) -> Result<Vec<Subject>, Error> {
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

struct EventDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> EventDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("event"),
            prefix: "event".to_string(),
        }
    }

    pub fn get_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<Event, Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let event = self.collection.get(&key)?;
        Ok(bincode::deserialize::<Event>(&event).unwrap())
    }

    pub fn get_events_by_range(
        &self,
        subject_id: &DigestIdentifier,
        from: Option<i64>,
        quantity: isize,
    ) -> Result<Vec<Event>, Error> {
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

    pub fn set_event(&self, subject_id: &DigestIdentifier, event: Event) -> Result<(), Error> {
        let sn = event.content.event_proposal.proposal.sn; // Preguntar si este sn es el último o el nuevo
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<Event>(&event) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
            Element::N(sn),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}

struct PrevalidatedEventDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> PrevalidatedEventDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("prevalidated-event"),
            prefix: "prevalidated-event".to_string(),
        }
    }

    pub fn get_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<Event, Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let prevalidated_event = self.collection.get(&key)?;
        Ok(bincode::deserialize::<Event>(&prevalidated_event).unwrap())
    }

    pub fn set_prevalidated_event(
        &self,
        subject_id: &DigestIdentifier,
        event: Event,
    ) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<Event>(&event) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}

struct RequestDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> RequestDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("request"),
            prefix: "request".to_string(),
        }
    }

    pub fn get_request(&self, subject_id: &DigestIdentifier) -> Result<EventRequest, Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let request = self.collection.get(&key)?;
        Ok(bincode::deserialize::<EventRequest>(&request).unwrap())
    }

    pub fn get_all_request(&self) -> Vec<EventRequest> {
        let mut result = Vec::new();
        for (_, request) in self.collection.iter(false, self.prefix.clone()) {
            let request = bincode::deserialize::<EventRequest>(&request).unwrap();
            result.push(request);
        }
        result
    }

    pub fn set_request(
        &self,
        subject_id: &DigestIdentifier,
        request: EventRequest,
    ) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<EventRequest>(&request) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_request(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}

struct ControllerIdDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> ControllerIdDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("controller-id"),
            prefix: "controller-id".to_string(),
        }
    }

    pub fn get_controller_id(&self) -> Result<String, Error> {
        let key_elements: Vec<Element> = vec![Element::S(self.prefix.clone())];
        let key = get_key(key_elements)?;
        let controller_id = self.collection.get(&key)?;
        Ok(bincode::deserialize::<String>(&controller_id).unwrap())
    }

    pub fn set_controller_id(&self, controller_id: String) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![Element::S(self.prefix.clone())];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<String>(&controller_id) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }
}

struct NotaryDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> NotaryDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("notary"),
            prefix: "notary".to_string(),
        }
    }

    pub fn get_notary_register(
        &self,
        owner: &KeyIdentifier,
        subject_id: &DigestIdentifier,
    ) -> Result<(DigestIdentifier, u64), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(owner.to_str()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let notary_register = self.collection.get(&key)?;
        Ok(bincode::deserialize::<(DigestIdentifier, u64)>(&notary_register).unwrap())
    }

    pub fn set_notary_register(
        &self,
        owner: &KeyIdentifier,
        subject_id: &DigestIdentifier,
        event_hash: DigestIdentifier,
        sn: u64,
    ) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(owner.to_str()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<(DigestIdentifier, u64)>(&(event_hash, sn)) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }
}

struct ContractDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> ContractDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("contract"),
            prefix: "contract".to_string(),
        }
    }

    pub fn get_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
    ) -> Result<(Vec<u8>, DigestIdentifier, u64), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(governance_id.to_str()),
            Element::S(schema_id.to_string()),
        ];
        let key = get_key(key_elements)?;
        let contract = self.collection.get(&key)?;
        Ok(bincode::deserialize::<(Vec<u8>, DigestIdentifier, u64)>(&contract).unwrap())
    }

    pub fn put_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
        contract: Vec<u8>,
        hash: DigestIdentifier,
        gov_version: u64,
    ) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(governance_id.to_str()),
            Element::S(schema_id.to_string()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<(Vec<u8>, DigestIdentifier, u64)>(&(contract, hash, gov_version)) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn get_governance_contract(&self) -> Result<Vec<u8>, Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S("governance".to_string()),
        ];
        let key = get_key(key_elements)?;
        let contract = self.collection.get(&key)?;
        let contract = bincode::deserialize::<(Vec<u8>, DigestIdentifier, u64)>(&contract).unwrap();
        Ok(contract.0)
    }

    pub fn put_governance_contract(&self, contract: Vec<u8>) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S("governance".to_string()),
        ];
        let key = get_key(key_elements)?;
        let Ok(data) = bincode::serialize::<(Vec<u8>, DigestIdentifier, u64)>(&(contract, DigestIdentifier::default(), 0)) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }
}

struct NotarySignaturesDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> NotarySignaturesDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("notary-signatures"),
            prefix: "notary-signatures".to_string(),
        }
    }

    pub fn get_notary_signatures(
        &self,
        subject_id: &DigestIdentifier,
        _sn: u64,
    ) -> Result<(u64, HashSet<NotaryEventResponse>), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let notary_signatures = self.collection.get(&key)?;
        Ok(
            bincode::deserialize::<(u64, HashSet<NotaryEventResponse>)>(&notary_signatures)
                .unwrap(),
        )
    }

    pub fn set_notary_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<NotaryEventResponse>,
    ) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let total_signatures = match self.collection.get(&key) {
            Ok(other) => {
                let other =
                    bincode::deserialize::<(u64, HashSet<NotaryEventResponse>)>(&other).unwrap();
                signatures.union(&other.1).cloned().collect()
            }
            Err(Error::EntryNotFound) => signatures,
            Err(error) => {
                return Err(error);
            }
        };
        let Ok(data) = bincode::serialize::<(u64, HashSet<NotaryEventResponse>)>(&(sn, total_signatures)) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn delete_notary_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}

struct WitnessSignaturesDb<C: DatabaseCollection> {
    collection: C,
    prefix: String,
}

impl<C: DatabaseCollection> WitnessSignaturesDb<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        Self {
            collection: manager.create_collection("witness-signatures"),
            prefix: "witness-signatures".to_string(),
        }
    }

    pub fn get_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<(u64, HashSet<Signature>), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let witness_signatures = self.collection.get(&key)?;
        Ok(bincode::deserialize::<(u64, HashSet<Signature>)>(&witness_signatures).unwrap())
    }

    pub fn get_all_witness_signatures(
        &self,
    ) -> Result<Vec<(DigestIdentifier, u64, HashSet<Signature>)>, Error> {
        // let key_elements: Vec<Element> = vec![Element::S(self.prefix.clone())];
        let iter = self.collection.iter(false, self.prefix.clone());
        Ok(iter
            .map(|ws| {
                let ws_1 = bincode::deserialize::<(u64, HashSet<Signature>)>(&ws.1).unwrap();
                (DigestIdentifier::from_str(&ws.0).unwrap(), ws_1.0, ws_1.1)
            })
            .collect())
    }

    pub fn set_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        let total_signatures = match self.collection.get(&key) {
            Ok(other) => {
                let other = bincode::deserialize::<(u64, HashSet<Signature>)>(&other).unwrap();
                signatures.union(&other.1).cloned().collect()
            }
            Err(Error::EntryNotFound) => signatures,
            Err(error) => {
                return Err(error);
            }
        };
        let Ok(data) = bincode::serialize::<(u64, HashSet<Signature>)>(&(sn, total_signatures)) else {
            return Err(Error::SerializeError);
        };
        self.collection.put(&key, data)
    }

    pub fn del_witness_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let key_elements: Vec<Element> = vec![
            Element::S(self.prefix.clone()),
            Element::S(subject_id.to_str()),
        ];
        let key = get_key(key_elements)?;
        self.collection.del(&key)
    }
}
*/
