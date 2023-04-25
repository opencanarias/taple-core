use std::collections::{HashMap, HashSet};

use json_patch::{patch, Patch};
use serde_json::Value;

use crate::{
    commons::models::state::Subject,
    database::DB,
    event_request::{EventRequest, EventRequestType},
    governance::{GovernanceAPI, GovernanceInterface},
    identifier::{DigestIdentifier, Derivable},
    signature::Signature,
    DatabaseManager, Event,
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

    pub async fn genesis(&mut self, event_request: EventRequest) -> Result<(), LedgerError> {
        // Añadir a subject_is_gov si es una governance y no está
        let EventRequestType::Create(create_request) = event_request.request.clone() else {
            return Err(LedgerError::StateInGenesis)
        };
        let governance_version = self
            .gov_api
            .get_governance_version(create_request.governance_id.clone())
            .await?;
        let init_state = self
            .gov_api
            .get_init_state(
                create_request.governance_id,
                create_request.schema_id.clone(),
                governance_version,
            )
            .await?;
        let init_state_string = serde_json::to_string(&init_state)
            .map_err(|_| LedgerError::ErrorParsingJsonString("Init State".to_owned()))?;
        // Crear sujeto a partir de genesis y evento
        let subject = Subject::from_genesis_request(event_request.clone(), init_state_string)
            .map_err(LedgerError::SubjectError)?;
        // Crear evento a partir de event_request
        let event = Event::from_genesis_request(
            event_request,
            subject.keys.clone().unwrap(),
            governance_version,
            &init_state,
        )
        .map_err(LedgerError::SubjectError)?;
        // Añadir sujeto y evento a base de datos
        let subject_id = subject.subject_id.clone();
        if &create_request.schema_id == "governance" {
            self.subject_is_gov.insert(subject_id.clone(), true);
            // Enviar mensaje a gov de governance updated con el id y el sn
        } else {
            self.subject_is_gov.insert(subject_id.clone(), false);
        }
        self.database
            .set_subject(&subject_id, subject)
            .map_err(|error| LedgerError::DatabaseError(error.to_string()))?;
        self.database
            .set_event(&subject_id, event)
            .map_err(|error| LedgerError::DatabaseError(error.to_string()))?;
        // Mandar subject_id y evento en mensaje
        // TODO
        todo!()
    }

    pub async fn event_validated(
        &mut self,
        subject_id: DigestIdentifier,
        event: Event,
        signatures: HashSet<Signature>,
    ) -> Result<(), LedgerError> {
        // Añadir a subject_is_gov si es una governance y no está
        self.database.set_signatures(
            &subject_id,
            event.content.event_proposal.proposal.sn,
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
        let json_patch = event.content.event_proposal.proposal.json_patch.as_str();
        let prev_properties = subject.properties.as_str();
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
                // TODO
            }
            Some(false) => {}
            None => {
                // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                self.subject_is_gov.insert(subject_id.clone(), false);
                if self.gov_api.is_governance(subject_id.clone()).await? {
                    self.subject_is_gov.insert(subject_id, true);
                    // Enviar mensaje a gov de governance updated con el id y el sn
                } else {
                    self.subject_is_gov.insert(subject_id, false);
                }
            }
        }
        todo!()
    }

    pub fn external_event(
        &self,
        event: Event,
        signatures: HashSet<Signature>,
    ) -> Result<(), LedgerError> {
        // Comprobar si es genesis o state
        match &event.content.event_proposal.proposal.event_request.request {
            EventRequestType::Create(create_request) => {
                // Comprobar que evaluation es None
                if event.content.event_proposal.proposal.evaluation.is_some() {
                    return Err(LedgerError::ErrorParsingJsonString(
                        "Evaluation should be None in external genesis event".to_owned(),
                    ));
                }
                // Comprobaciones criptográficas
                let subject_id = match DigestIdentifier::from_serializable_borsh((
                    event
                        .content
                        .event_proposal
                        .proposal
                        .event_request
                        .signature
                        .content
                        .event_content_hash,
                    event
                        .content
                        .event_proposal
                        .proposal
                        .event_request
                        .signature
                        .content
                        .signer
                        .public_key, // No estoy seguro que esto equivalga al vector de bytes pero creo que si
                )) {
                    Ok(subject_id) => subject_id,
                    Err(_) => {
                        return Err(LedgerError::CryptoError(
                            "Error creating subject_id in external event".to_owned(),
                        ))
                    }
                };
                match self.database.get_subject(&subject_id) {
                    Ok(_) => {
                        return Err(LedgerError::SubjectAlreadyExists(
                            subject_id.to_str().to_owned(),
                        ))
                    }
                    Err(crate::DbError::EntryNotFound) => {}
                    Err(error) => {
                        return Err(LedgerError::DatabaseError(error.to_string()));
                    }
                };
            }
            EventRequestType::State(state_request) => {
                // Comprobaciones criptográficas
            }
        }
        todo!();
    }

    pub fn external_intermediate_event(&self, event: Event) -> Result<(), LedgerError> {
        // Comprobar si es genesis o state
        todo!();
    }
}
