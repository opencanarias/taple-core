use std::collections::{HashMap, HashSet};

use crate::{
    database::DB, event_request::EventRequest, governance::GovernanceAPI,
    identifier::DigestIdentifier, signature::Signature, DatabaseManager, Event,
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

    pub fn event_prevalidated(
        &self,
        subject_id: DigestIdentifier,
        event: Event,
    ) -> Result<(), LedgerError> {
        // Añadir a subject_is_gov si es una governance y no está
        self.database.set_event(&subject_id, event);
        if self.gov_api.is_governance(subject_id.clone()) {
            self.subject_is_gov.insert(subject_id, true);
        } else {
            self.subject_is_gov.insert(subject_id, false);
        }
        todo!()
    }

    pub fn genesis(&self, event_request: EventRequest) -> Result<(), LedgerError> {
        // Añadir a subject_is_gov si es una governance y no está
        // Crear evento a partir de event_request
        // Crear sujeto a partir de genesis y evento
        // Añadir sujeto y evento a base de datos
        todo!()
    }

    pub fn event_validated(
        &self,
        subject_id: DigestIdentifier,
        event: Event,
        signatures: HashSet<Signature>,
    ) -> Result<(), LedgerError> {
        // Añadir a subject_is_gov si es una governance y no está
        self.database.set_signatures(
            &subject_id,
            &event.content.event_proposal.proposal.sn,
            signatures,
        );
        let is_gov = self.subject_is_gov.get(&subject_id);
        // Aplicar event sourcing
        
        match is_gov {
            Some(true) => {
                // Enviar mensaje a gov de governance updated con el id y el sn
            }
            Some(false) => {}
            None => {
                // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                self.subject_is_gov.insert(subject_id, false);
                if self.gov_api.is_governance(subject_id.clone()) {
                    self.subject_is_gov.insert(subject_id, true);
                    // Enviar mensaje a gov de governance updated con el id y el sn
                } else {
                    self.subject_is_gov.insert(subject_id, false);
                }
            }
        }
        // Si es gov enviar mensaje a gov de governance updated con el id y el sn
        todo!()
    }
}
