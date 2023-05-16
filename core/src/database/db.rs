use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::commons::models::notary::NotaryEventResponse;
use crate::commons::models::state::Subject;
use crate::event_request::EventRequest;
use crate::identifier::{Derivable, DigestIdentifier, KeyIdentifier};
use crate::signature::Signature;
use crate::Event;

use super::error::Error;
use super::{DatabaseCollection, DatabaseManager};

const SIGNATURE_TABLE: &str = "signature";
const SUBJECT_TABLE: &str = "subject";
const EVENT_TABLE: &str = "event";
const PREVALIDATED_EVENT_TABLE: &str = "prevalidated-event";
const REQUEST_TABLE: &str = "request";
const ID_TABLE: &str = "controller-id";
const NOTARY_TABLE: &str = "notary";
const CONTRACT_TABLE: &str = "contract";
const NOTARY_SIGNATURES: &str = "notary-signatures";
const WITNESS_SIGNATURES: &str = "witness-signatures";
const SUBJECTS_BY_GOVERNANCE: &str = "governance-index";

pub struct DB<M: DatabaseManager> {
    _manager: Arc<M>,
    signature_db: Box<dyn DatabaseCollection<InnerDataType = HashSet<Signature>>>,
    subject_db: Box<dyn DatabaseCollection<InnerDataType = Subject>>,
    event_db: Box<dyn DatabaseCollection<InnerDataType = Event>>,
    prevalidated_event_db: Box<dyn DatabaseCollection<InnerDataType = Event>>,
    request_db: Box<dyn DatabaseCollection<InnerDataType = EventRequest>>,
    id_db: Box<dyn DatabaseCollection<InnerDataType = String>>,
    notary_db: Box<dyn DatabaseCollection<InnerDataType = (DigestIdentifier, u64)>>,
    contract_db: Box<dyn DatabaseCollection<InnerDataType = (Vec<u8>, DigestIdentifier, u64)>>,
    notary_signatures_db:
        Box<dyn DatabaseCollection<InnerDataType = (u64, HashSet<NotaryEventResponse>)>>,
    witness_signatures_db: Box<dyn DatabaseCollection<InnerDataType = (u64, HashSet<Signature>)>>,
    subjects_by_governance: Box<dyn DatabaseCollection<InnerDataType = DigestIdentifier>>,
}

impl<M: DatabaseManager> DB<M> {
    pub fn new(manager: Arc<M>) -> Self {
        let signature_db = manager.create_collection(SIGNATURE_TABLE);
        let subject_db = manager.create_collection(SUBJECT_TABLE);
        let event_db = manager.create_collection(EVENT_TABLE);
        let prevalidated_event_db = manager.create_collection(PREVALIDATED_EVENT_TABLE);
        let request_db = manager.create_collection(REQUEST_TABLE);
        let id_db = manager.create_collection(ID_TABLE);
        let notary_db = manager.create_collection(NOTARY_TABLE);
        let contract_db = manager.create_collection(CONTRACT_TABLE);
        let notary_signatures_db = manager.create_collection(NOTARY_SIGNATURES);
        let witness_signatures_db = manager.create_collection(WITNESS_SIGNATURES);
        let subjects_by_governance = manager.create_collection(SUBJECTS_BY_GOVERNANCE);
        Self {
            _manager: manager,
            signature_db,
            subject_db,
            event_db,
            prevalidated_event_db,
            request_db,
            id_db,
            notary_db,
            contract_db,
            notary_signatures_db,
            witness_signatures_db,
            subjects_by_governance,
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
                iter.next(); // Exclusive From
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
        from: Option<i64>,
        quantity: isize,
    ) -> Result<Vec<Event>, Error> {
        let id = subject_id.to_str();
        let from = match from {
            Some(from) => Some(from.to_string()),
            None => None,
        };
        let events_by_subject = self.event_db.partition(&id);
        self.get_by_range(from, quantity, &events_by_subject)
    }

    pub fn set_event(&self, subject_id: &DigestIdentifier, event: Event) -> Result<(), Error> {
        // TODO: DETERMINAR SI DEVOLVER RESULT
        let id = subject_id.to_str();
        let events_by_subject = self.event_db.partition(&id);
        let sn = event.content.event_proposal.proposal.sn.to_string();
        events_by_subject.put(&sn, event)
    }

    pub fn del_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), Error> {
        let id = subject_id.to_str();
        let events_by_subject = self.event_db.partition(&id);
        let sn = sn.to_string();
        events_by_subject.del(&sn)
    }

    pub fn get_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<Event, Error> {
        let id = subject_id.to_str();
        self.prevalidated_event_db.get(&id)
    }

    pub fn set_prevalidated_event(
        &self,
        subject_id: &DigestIdentifier,
        event: Event,
    ) -> Result<(), Error> {
        // TODO: DETERMINAR SI DEVOLVER RESULT
        let id = subject_id.to_str();
        self.prevalidated_event_db.put(&id, event)
    }

    pub fn del_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let id = subject_id.to_str();
        self.prevalidated_event_db.del(&id)
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

    pub fn get_all_witness_signatures(
        &self,
    ) -> Result<Vec<(DigestIdentifier, u64, HashSet<Signature>)>, Error> {
        let iter = self.witness_signatures_db.iter();
        Ok(iter
            .map(|ws| (DigestIdentifier::from_str(&ws.0).unwrap(), ws.1 .0, ws.1 .1))
            .collect())
    }

    pub fn get_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<(u64, HashSet<Signature>), Error> {
        let id = subject_id.to_str();
        self.witness_signatures_db.get(&id)
    }

    pub fn get_notary_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Result<(u64, HashSet<NotaryEventResponse>), Error> {
        let id = subject_id.to_str();
        self.notary_signatures_db.get(&id)
    }

    pub fn delete_notary_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.notary_signatures_db.del(&subject_id.to_str())
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
        let total_signatures = match signatures_by_subject.get(&sn) {
            Ok(other) => signatures.union(&other).cloned().collect(),
            Err(Error::EntryNotFound) => signatures,
            Err(error) => {
                // logError!("Error detected in database get_event operation: {}", error);
                return Err(error);
            }
        };
        signatures_by_subject.put(&sn.to_string(), total_signatures)
    }

    pub fn del_signatures(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), Error> {
        let id = subject_id.to_str();
        let signatures_by_subject = self.signature_db.partition(&id);
        let sn = sn.to_string();
        signatures_by_subject.del(&sn)
    }

    pub fn set_notary_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<NotaryEventResponse>,
    ) -> Result<(), Error> {
        let id = subject_id.to_str();
        let total_signatures = match self.notary_signatures_db.get(&id) {
            Ok((_u, other)) => signatures.union(&other).cloned().collect(),
            Err(Error::EntryNotFound) => signatures,
            Err(error) => {
                // logError!("Error detected in database get_event operation: {}", error);
                return Err(error);
            }
        };
        self.notary_signatures_db.put(&id, (sn, total_signatures))
    }

    pub fn set_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), Error> {
        let id = subject_id.to_str();
        let total_signatures = match self.witness_signatures_db.get(&id) {
            Ok((_, other)) => signatures.union(&other).cloned().collect(),
            Err(Error::EntryNotFound) => signatures,
            Err(error) => {
                // logError!("Error detected in database get_event operation: {}", error);
                return Err(error);
            }
        };
        self.witness_signatures_db.put(&id, (sn, total_signatures))
    }

    pub fn del_witness_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let id = subject_id.to_str();
        self.witness_signatures_db.del(&id)
    }

    pub fn get_subject(&self, subject_id: &DigestIdentifier) -> Result<Subject, Error> {
        self.subject_db.get(&subject_id.to_str())
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
        let id = subject_id.to_str();
        self.subject_db.put(&id, subject)
    }

    pub fn del_subject(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let id = subject_id.to_str();
        self.subject_db.del(&id)
    }

    // pub fn apply_event_sourcing(&self, event_content: &EventContent) -> Result<(), Error> {
    //     // TODO: Consultar sobre si este método debería existir
    //     let subject_id = &event_content.subject_id;
    //     let mut subject = self.get_subject(&subject_id)?;
    //     subject
    //         .apply(event_content)
    //         .map_err(|_| Error::SubjectApplyFailed)?;
    //     // Persist the change
    //     self.set_subject(subject_id, subject)?;
    //     let id = subject_id.to_str();
    //     let signatures_by_subject = self.signature_db.partition(&id);
    //     match signatures_by_subject.del(&(event_content.sn - 1).to_string()) {
    //         Ok(_) | Err(Error::EntryNotFound) => Ok(()),
    //         Err(error) => Err(error),
    //     }
    // }

    // pub fn get_all_heads(&self) -> Result<HashMap<DigestIdentifier, LedgerState>, Error> {
    //     let mut result = HashMap::new();
    //     for (key, subject) in self.subject_db.iter() {
    //         let subject_id =
    //             DigestIdentifier::from_str(&key).map_err(|_| Error::NoDigestIdentifier)?;
    //         result.insert(subject_id, subject.ledger_state);
    //     }
    //     Ok(result)
    // }

    pub fn get_all_subjects(&self) -> Vec<Subject> {
        let mut result = Vec::new();
        for (a, subject) in self.subject_db.iter() {
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

    pub fn get_request(&self, subject_id: &DigestIdentifier) -> Result<EventRequest, Error> {
        let id = subject_id.to_str();
        self.request_db.get(&id)
    }

    pub fn set_request(
        &self,
        subject_id: &DigestIdentifier,
        request: EventRequest,
    ) -> Result<(), Error> {
        let id = subject_id.to_str();
        self.request_db.put(&id, request)
    }

    pub fn del_request(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        let id = subject_id.to_str();
        self.request_db.del(&id)
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
    ) -> Result<(Vec<u8>, DigestIdentifier, u64), Error> {
        let id = governance_id.to_str();
        let schemas_by_governances = self.contract_db.partition(&id);
        schemas_by_governances.get(schema_id)
    }

    pub fn get_governance_contract(&self) -> Result<Vec<u8>, Error> {
        let contract = self.contract_db.get("governance");
        match contract {
            Ok(result) => Ok(result.0),
            Err(error) => Err(error),
        }
    }

    pub fn put_governance_contract(&self, contract: Vec<u8>) -> Result<(), Error> {
        self.contract_db
            .put("governance", (contract, DigestIdentifier::default(), 0))
    }

    pub fn put_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
        contract: Vec<u8>,
        hash: DigestIdentifier,
        gov_version: u64,
    ) -> Result<(), Error> {
        let id = governance_id.to_str();
        let schemas_by_governances = self.contract_db.partition(&id);
        schemas_by_governances.put(schema_id, (contract, hash, gov_version))
    }
}
