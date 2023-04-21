use std::collections::{HashMap, HashSet};

use json_patch::{patch, Patch};
use serde_json::Value;

use crate::{
    database::DB, event_request::{EventRequest, EventRequestType}, governance::GovernanceAPI,
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

    pub fn genesis(&self, event_request: EventRequest) -> Result<(), LedgerError> {
        // Añadir a subject_is_gov si es una governance y no está
        let EventRequestType::Create(create_request) = event_request.request else {
            return Err(LedgerError::StateInGenesis)
        };
        // Crear evento a partir de event_request
        // Crear sujeto a partir de genesis y evento
        // Añadir sujeto y evento a base de datos
        // Mandar subject_id y evento en mensaje
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
        // Aplicar event sourcing
        let mut subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => LedgerError::SubjectNotFound(subject_id.to_str()),
                _ => LedgerError::DatabaseError(error.to_string()),
            })?;
        let json_patch = event.content.event_proposal.proposal.json_patch.to_str();
        let prev_properties = subject.properties.to_str();
        let Ok(patch_json) = serde_json::from_str::<Patch>(json_patch) else {
                return Err(LedgerError::ErrorParsingJsonString(json_patch.to_owned()));
            };
        let Ok(mut state) = serde_json::from_str::<Value>(prev_properties) else {
                return Err(LedgerError::ErrorParsingJsonString(prev_properties.to_owned()));
            };
        let Ok(()) = patch(&mut state, &patch_json) else {
                return Err(LedgerError::ErrorApplyingPatch(json_patch.to_owned()));
            };
        let state = serde_json::to_string(&state)
            .map_err(|_| LedgerError::ErrorParsingJsonString("New State after patch".to_owned()))?;
        subject.sn = event.content.event_proposal.proposal.sn;
        subject.properties = state;
        self.database
            .set_subject(&subject_id, subject)
            .map_err(|error| LedgerError::DatabaseError(error.to_string()))?;
        // Comprobar is_gov
        let is_gov = self.subject_is_gov.get(&subject_id);
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
