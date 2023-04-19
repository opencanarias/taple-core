use std::collections::{HashMap, HashSet};

use crate::{
    database::DB, event_request::EventRequest, governance::GovernanceAPI, signature::Signature,
    DatabaseManager, Event, identifier::DigestIdentifier,
};

use super::errors::LedgerError;

pub struct Ledger<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    subject_is_gov: HashMap<DigestIdentifier, bool>,
}

impl<D: DatabaseManager> Ledger<D> {
    pub fn new(gov_api: GovernanceAPI, database: DB<D>) -> Self {
        Self {
            gov_api,
            database,
            subject_is_gov: HashMap::new(),
        }
    }

    pub fn init(&self) -> Result<(), LedgerError> {
        todo!()
    }

    pub fn event_prevalidated(&self, event: Event) -> Result<(), LedgerError> {
        self.database.add_event(event);
        todo!()
    }

    pub fn genesis(&self, event_request: EventRequest) -> Result<(), LedgerError> {
        todo!()
    }

    pub fn event_validated(
        &self,
        event: Event,
        signatures: HashSet<Signature>,
    ) -> Result<(), LedgerError> {
        todo!()
    }
}
