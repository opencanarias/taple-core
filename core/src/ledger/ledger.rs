use std::collections::{HashMap, HashSet};

use json_patch::{patch, Patch};
use serde_json::Value;

use crate::{
    commons::models::state::Subject,
    database::DB,
    event_content::Metadata,
    event_request::{EventRequest, EventRequestType},
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    signature::Signature,
    DatabaseManager, Event,
};

use super::errors::LedgerError;

pub struct LedgerState {
    pub current_sn: u64,
    pub head: Option<u64>,
}

pub struct Ledger<D: DatabaseManager> {
    gov_api: GovernanceAPI,
    database: DB<D>,
    subject_is_gov: HashMap<DigestIdentifier, bool>,
    ledger_state: HashMap<DigestIdentifier, LedgerState>,
}

impl<D: DatabaseManager> Ledger<D> {
    pub fn new(gov_api: GovernanceAPI, database: DB<D>) -> Self {
        Self {
            gov_api,
            database,
            subject_is_gov: HashMap::new(),
            ledger_state: HashMap::new(),
        }
    }

    pub async fn init(&mut self) -> Result<(), LedgerError> {
        // Revisar si tengo sujetos a medio camino entre estado actual y LCE
        // Actualizar hashmaps
        let subjects = self.database.get_all_subjects();
        for subject in subjects.into_iter() {
            // Añadirlo a is_gov
            if self
                .gov_api
                .is_governance(subject.subject_id.clone())
                .await?
            {
                self.subject_is_gov.insert(subject.subject_id.clone(), true);
                // Enviar mensaje a gov de governance updated con el id y el sn
            } else {
                self.subject_is_gov
                    .insert(subject.subject_id.clone(), false);
            }
            // Actualizar ledger_state para ese subject
            let mut last_two_events =
                self.database
                    .get_events_by_range(&subject.subject_id, Some(-1), 2)?;
            let last_event = match last_two_events.pop() {
                Some(event) => event,
                None => return Err(LedgerError::ZeroEventsSubject(subject.subject_id.to_str())),
            };
            let pre_last_event = match last_two_events.pop() {
                Some(event) => event,
                None => {
                    self.ledger_state.insert(
                        subject.subject_id,
                        LedgerState {
                            current_sn: 0,
                            head: None,
                        },
                    );
                    continue;
                }
            };
            if last_event.content.event_proposal.proposal.sn
                == pre_last_event.content.event_proposal.proposal.sn + 1
            {
                if subject.sn != last_event.content.event_proposal.proposal.sn {
                    return Err(LedgerError::WrongSnInSubject(subject.subject_id.to_str()));
                }
                self.ledger_state.insert(
                    subject.subject_id,
                    LedgerState {
                        current_sn: last_event.content.event_proposal.proposal.sn,
                        head: None,
                    },
                );
            } else {
                if subject.sn != pre_last_event.content.event_proposal.proposal.sn {
                    return Err(LedgerError::WrongSnInSubject(subject.subject_id.to_str()));
                }
                self.ledger_state.insert(
                    subject.subject_id,
                    LedgerState {
                        current_sn: pre_last_event.content.event_proposal.proposal.sn,
                        head: Some(last_event.content.event_proposal.proposal.sn),
                    },
                );
            }
        }
        Ok(())
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
        self.database.set_subject(&subject_id, subject)?;
        self.database.set_event(&subject_id, event)?;
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
        self.database.set_signatures(
            &subject_id,
            event.content.event_proposal.proposal.sn,
            signatures,
        )?;
        // Aplicar event sourcing
        let mut subject = self
            .database
            .get_subject(&subject_id)
            .map_err(|error| match error {
                crate::DbError::EntryNotFound => LedgerError::SubjectNotFound(subject_id.to_str()),
                _ => LedgerError::DatabaseError(error),
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
            .map_err(|error| LedgerError::DatabaseError(error))?;
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

    pub async fn external_event(
        &self,
        event: Event,
        signatures: HashSet<Signature>,
    ) -> Result<(), LedgerError> {
        // Comprobaciones criptográficas
        event.check_signatures()?;
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
                    &event
                        .content
                        .event_proposal
                        .proposal
                        .event_request
                        .signature
                        .content
                        .event_content_hash,
                    &event
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
                        return Err(LedgerError::DatabaseError(error));
                    }
                };
                let invoker = event
                    .content
                    .event_proposal
                    .proposal
                    .event_request
                    .signature
                    .content
                    .signer
                    .clone();
                let metadata = Metadata {
                    namespace: create_request.namespace.clone(),
                    subject_id: subject_id.clone(),
                    governance_id: create_request.governance_id.clone(),
                    governance_version: event.content.event_proposal.proposal.gov_version,
                    schema_id: create_request.schema_id.clone(),
                    owner: invoker.clone(),
                    creator: invoker.clone(),
                };
                // Ignoramos las firmas por ahora
                // Comprobar que el creador tiene permisos de creación
                let creation_roles = self
                    .gov_api
                    .get_signers(metadata.clone(), ValidationStage::Create)
                    .await?;
                if !creation_roles.contains(&invoker) {
                    return Err(LedgerError::Unauthorized("Crreator not allowed".into()));
                } // TODO: No estamos comprobando que pueda ser un external el que cree el subject y lo permitamos si tenia permisos.
                  // Crear sujeto y añadirlo a base de datos
                let init_state = self
                    .gov_api
                    .get_init_state(
                        metadata.governance_id,
                        metadata.schema_id,
                        metadata.governance_version,
                    )
                    .await?;
                let init_state = serde_json::to_string(&init_state)
                    .map_err(|_| LedgerError::ErrorParsingJsonString("Init state".to_owned()))?;
                let subject = Subject::from_genesis_event(event, init_state)?;
                self.database.set_subject(&subject_id, subject)?;
                // Enviar mensaje a distribution manager
                todo!();
            }
            EventRequestType::State(state_request) => {
                // Comprobaciones criptográficas
                let subject = match self.database.get_subject(&state_request.subject_id) {
                    Ok(subject) => subject,
                    Err(crate::DbError::EntryNotFound) => {
                        return Err(LedgerError::SubjectNotFound("".into()));
                    }
                    Err(error) => {
                        return Err(LedgerError::DatabaseError(error));
                    }
                };
                // Comprobar que el invokator es válido
                let invoker = event
                    .content
                    .event_proposal
                    .proposal
                    .event_request
                    .signature
                    .content
                    .signer
                    .clone();
                // TODO: Pedir  invokadores válidos a la gov
                // Comprobar que las firmas son válidas y suficientes
                let metadata = Metadata {
                    namespace: subject.namespace,
                    subject_id: subject.subject_id,
                    governance_id: subject.governance_id,
                    governance_version: event.content.event_proposal.proposal.gov_version,
                    schema_id: subject.schema_id,
                    owner: subject.owner,
                    creator: subject.creator,
                };
                let (signers, quorum) = self
                    .get_signers_and_quorum(metadata, ValidationStage::Validate)
                    .await?;
                verify_signatures(
                    &signatures,
                    &signers,
                    quorum,
                    &event.signature.content.event_content_hash,
                )?;
                // Comprobar si es evento siguiente o LCE
                if event.content.event_proposal.proposal.sn == subject.sn + 1 {
                    // Caso Evento Siguiente
                } else if event.content.event_proposal.proposal.sn > subject.sn + 1 {
                    // Caso LCE
                } else {
                    // Caso evento repetido
                    return Err(LedgerError::EventAlreadyExists);
                }
            }
        }
        todo!();
    }

    pub fn external_intermediate_event(&self, event: Event) -> Result<(), LedgerError> {
        // Comprobaciones criptográficas
        event.check_signatures()?;
        // Comprobar si es genesis o state
        match &event.content.event_proposal.proposal.event_request.request {
            EventRequestType::Create(create_request) => {
                // En principio los genesis van a venir por aquí porque no incluyen firmas hasta que lo cambiemos
            }
            EventRequestType::State(state_request) => {}
        }
        // Comprobar que tengo firmas de un evento mayor y que es el evento siguiente que necesito para este subject
        todo!();
    }

    // TODO Existe otra igual en event manager, unificar en una sola y poner en utils
    async fn get_signers_and_quorum(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<(HashSet<KeyIdentifier>, u32), LedgerError> {
        let signers = self
            .gov_api
            .get_signers(metadata.clone(), stage.clone())
            .await?;
        let quorum_size = self.gov_api.get_quorum(metadata, stage).await?;
        Ok((signers, quorum_size))
    }
}

fn verify_signatures(
    signatures: &HashSet<Signature>,
    signers: &HashSet<KeyIdentifier>,
    quorum_size: u32,
    event_hash: &DigestIdentifier,
) -> Result<(), LedgerError> {
    let mut actual_signers = HashSet::new();
    for signature in signatures.iter() {
        let signer = signature.content.signer.clone();
        if &signature.content.event_content_hash != event_hash {
            log::error!("Invalid Event Hash in Signature");
            continue;
        }
        match signature.verify() {
            Ok(_) => (),
            Err(_) => {
                log::error!("Invalid Signature Detected");
                continue;
            }
        }
        if !signers.contains(&signer) {
            log::error!("Signer {} not allowed", signer.to_str());
            continue;
        }
        if !actual_signers.insert(signer.clone()) {
            log::error!(
                "Signer {} in more than one validation signature",
                signer.to_str()
            );
            continue;
        }
    }
    if actual_signers.len() < quorum_size as usize {
        log::error!(
            "Not enough signatures. Expected: {}, Actual: {}",
            quorum_size,
            actual_signers.len()
        );
        return Err(LedgerError::NotEnoughSignatures(event_hash.to_str()));
    }
    Ok(())
}
