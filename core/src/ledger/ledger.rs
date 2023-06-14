use std::collections::{hash_map::Entry, HashMap, HashSet};

use crate::commons::crypto::KeyGenerator;
use crate::commons::models::state::generate_subject_id;
use crate::request::{RequestState, TapleRequest};
use crate::{
    commons::{
        channel::SenderEnd,
        models::{
            approval::Approval,
            event::ValidationProof,
            event_preevaluation::{Context, EventPreEvaluation},
            state::Subject,
        },
    },
    crypto::{Ed25519KeyPair, KeyMaterial, KeyPair},
    database::{Error as DbError, DB},
    distribution::{error::DistributionErrorResponses, DistributionMessagesNew},
    event_content::Metadata,
    event_request::{EventRequest, EventRequestType},
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
    signature::Signature,
    utils::message::ledger::{request_event, request_gov_event},
    DatabaseCollection, Event,
};

use super::errors::LedgerError;

#[derive(Debug, Clone)]
pub struct LedgerState {
    pub current_sn: Option<u64>,
    pub head: Option<u64>,
}

pub struct Ledger<C: DatabaseCollection> {
    gov_api: GovernanceAPI,
    database: DB<C>,
    subject_is_gov: HashMap<DigestIdentifier, bool>,
    ledger_state: HashMap<DigestIdentifier, LedgerState>,
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    distribution_channel:
        SenderEnd<DistributionMessagesNew, Result<(), DistributionErrorResponses>>,
    our_id: KeyIdentifier,
}

impl<C: DatabaseCollection> Ledger<C> {
    pub fn new(
        gov_api: GovernanceAPI,
        database: DB<C>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        distribution_channel: SenderEnd<
            DistributionMessagesNew,
            Result<(), DistributionErrorResponses>,
        >,
        our_id: KeyIdentifier,
    ) -> Self {
        Self {
            gov_api,
            database,
            subject_is_gov: HashMap::new(),
            ledger_state: HashMap::new(),
            message_channel,
            distribution_channel,
            our_id,
        }
    }

    // async fn init_preautorized(&mut self) -> Result<(), LedgerError> {
    //     let data = self.database.get_all_keys()?;
    //     for (subject_id, _providers) in data {
    //         // All expecting subjects should be preauthorized
    //         match self
    //             .database
    //             .get_preauthorized_subject_and_providers(&subject_id)
    //         {
    //             Ok(_) => {}
    //             Err(DbError::EntryNotFound) => {
    //                 // Añadimos sujeto como preautorizado
    //                 self.database
    //                     .set_preauthorized_subject_and_providers(&subject_id, HashSet::new())?;
    //             }
    //             Err(error) => return Err(LedgerError::DatabaseError(error)),
    //         }
    //     }
    //     Ok(())
    // }

    pub async fn init(&mut self) -> Result<(), LedgerError> {
        // Revisamos posibles sujetos a recibir en transferencias sin preautorizar
        // self.init_preautorized().await?;
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
            let (last_event, pre_last_event) = {
                let mut last_two_events =
                    self.database
                        .get_events_by_range(&subject.subject_id, None, -2)?;
                if last_two_events.is_empty() {
                    return Err(LedgerError::ZeroEventsSubject(subject.subject_id.to_str()));
                }
                if last_two_events.len() == 1 {
                    self.ledger_state.insert(
                        subject.subject_id,
                        LedgerState {
                            current_sn: Some(0),
                            head: None,
                        },
                    );
                    continue;
                }
                let pre_last_event = last_two_events.pop().unwrap();
                let las_event = last_two_events.pop().unwrap();
                (las_event, pre_last_event)
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
                        current_sn: Some(last_event.content.event_proposal.proposal.sn),
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
                        current_sn: Some(pre_last_event.content.event_proposal.proposal.sn),
                        head: Some(last_event.content.event_proposal.proposal.sn),
                    },
                );
            }
        }
        log::warn!("TENGO {} sujetos pendietes", self.ledger_state.len());
        Ok(())
    }

    fn set_finished_request(
        &self,
        request_id: &DigestIdentifier,
        event_request: EventRequest,
        sn: u64,
        subject_id: DigestIdentifier,
    ) -> Result<(), LedgerError> {
        let mut taple_request: TapleRequest = event_request.clone().try_into()?;
        taple_request.sn = Some(sn);
        taple_request.subject_id = Some(subject_id.clone());
        taple_request.state = RequestState::Finished;
        self.database
            .set_taple_request(&request_id, &taple_request)?;
        Ok(())
    }

    pub async fn genesis(&mut self, event_request: EventRequest) -> Result<(), LedgerError> {
        let request_id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        // Añadir a subject_is_gov si es una governance y no está
        let EventRequestType::Create(create_request) = event_request.request.clone() else {
            return Err(LedgerError::StateInGenesis)
        };
        let governance_version = if create_request.schema_id == "governance"
            && create_request.governance_id.digest.is_empty()
        {
            0
        } else {
            self.gov_api
                .get_governance_version(
                    create_request.governance_id.clone(),
                    DigestIdentifier::default(),
                )
                .await?
        };
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
            event_request.clone(),
            subject.keys.clone().unwrap(),
            governance_version,
            &init_state,
        )
        .map_err(LedgerError::SubjectError)?;
        let sn = event.content.event_proposal.proposal.sn;
        // Añadir sujeto y evento a base de datos
        let subject_id = subject.subject_id.clone();
        if &create_request.schema_id == "governance" {
            self.subject_is_gov.insert(subject_id.clone(), true);
            // Enviar mensaje a gov de governance updated con el id y el sn
            self.gov_api
                .governance_updated(subject_id.clone(), sn)
                .await?;
        } else {
            self.subject_is_gov.insert(subject_id.clone(), false);
        }
        self.database
            .set_governance_index(&subject_id, &subject.governance_id)?;
        self.database.set_subject(&subject_id, subject)?;
        self.set_finished_request(&request_id, event_request, sn, subject_id.clone())?;
        self.database.set_event(&subject_id, event)?;
        // Actualizar Ledger State
        match self.ledger_state.entry(subject_id.clone()) {
            Entry::Occupied(mut ledger_state) => {
                let ledger_state = ledger_state.get_mut();
                ledger_state.current_sn = Some(0);
            }
            Entry::Vacant(entry) => {
                entry.insert(LedgerState {
                    current_sn: Some(0),
                    head: None,
                });
            }
        }
        // Mandar subject_id y evento en mensaje a distribution manager
        self.distribution_channel
            .tell(DistributionMessagesNew::SignaturesNeeded { subject_id, sn: 0 })
            .await?;
        Ok(())
    }

    pub async fn generate_key(&self) -> Result<KeyIdentifier, LedgerError> {
        // Generar material criptográfico y guardarlo en BBDD asociado al subject_id
        // TODO: Hacer la eleccion del MC dinámica. Es necesario primero hacer el cambio a nivel de state.rs
        let keys = KeyPair::Ed25519(Ed25519KeyPair::new());
        let public_key = KeyIdentifier::new(keys.get_key_derivator(), &keys.public_key_bytes());
        self.database.set_keys(&public_key, keys)?;
        Ok(public_key)

        // Así mismo, ponemos el sujeto como preautorizado
        // self.database
        //     .set_preauthorized_subject_and_providers(&subject_id, HashSet::new())?;
        // Ok(public_key)
    }

    pub async fn event_validated(
        &mut self,
        event: Event,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    ) -> Result<(), LedgerError> {
        let event_request = event.content.event_proposal.proposal.event_request.clone();
        let request_id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        let sn = event.content.event_proposal.proposal.sn;
        let subject_id = match &event_request.request {
            EventRequestType::Fact(state_request) => {
                let subject_id = state_request.subject_id.clone();
                // Aplicar event sourcing
                let mut subject =
                    self.database
                        .get_subject(&subject_id)
                        .map_err(|error| match error {
                            crate::DbError::EntryNotFound => {
                                LedgerError::SubjectNotFound(subject_id.to_str())
                            }
                            _ => LedgerError::DatabaseError(error),
                        })?;
                self.database.set_signatures(
                    &subject_id,
                    event.content.event_proposal.proposal.sn,
                    signatures,
                    validation_proof, // Current Owner
                )?;
                let json_patch = event.content.event_proposal.proposal.json_patch.as_str();
                subject.update_subject(json_patch, event.content.event_proposal.proposal.sn)?;
                self.database.set_event(&subject_id, event.clone())?;
                self.database.set_subject(&subject_id, subject)?;
                // Comprobar is_gov
                let is_gov = self.subject_is_gov.get(&subject_id);
                match is_gov {
                    Some(true) => {
                        // Enviar mensaje a gov de governance updated con el id y el sn
                        self.gov_api
                            .governance_updated(subject_id.clone(), sn)
                            .await?;
                    }
                    Some(false) => {
                        self.database.del_signatures(&subject_id, sn - 1)?;
                    }
                    None => {
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        if self.gov_api.is_governance(subject_id.clone()).await? {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // Enviar mensaje a gov de governance updated con el id y el sn
                            self.gov_api
                                .governance_updated(subject_id.clone(), sn)
                                .await?;
                        } else {
                            self.subject_is_gov.insert(subject_id.clone(), false);
                            self.database.del_signatures(&subject_id, sn - 1)?;
                        }
                    }
                }
                state_request.subject_id.clone()
            }
            EventRequestType::Create(_) => return Err(LedgerError::StateInGenesis),
            EventRequestType::Transfer(transfer_request) => {
                let subject_id = transfer_request.subject_id.clone();
                // Aplicar event sourcing
                let mut subject =
                    self.database
                        .get_subject(&subject_id)
                        .map_err(|error| match error {
                            crate::DbError::EntryNotFound => {
                                LedgerError::SubjectNotFound(subject_id.to_str())
                            }
                            _ => LedgerError::DatabaseError(error),
                        })?;
                self.database.set_signatures(
                    &subject_id,
                    event.content.event_proposal.proposal.sn,
                    signatures,
                    validation_proof,
                )?;
                self.database.set_event(&subject_id, event.clone())?;
                // Cambiar clave pública del sujeto y eliminar material criptográfico
                subject.public_key = transfer_request.public_key.clone();
                subject.owner = event
                    .content
                    .event_proposal
                    .proposal
                    .event_request
                    .signature
                    .content
                    .signer
                    .clone();
                subject.keys = None;
                subject.sn = event.content.event_proposal.proposal.sn;
                self.database.set_subject(&subject_id, subject)?;
                let is_gov = self.subject_is_gov.get(&subject_id);
                match is_gov {
                    Some(true) => {}
                    Some(false) => {
                        self.database.del_signatures(&subject_id, sn - 1)?;
                    }
                    None => {
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        if self.gov_api.is_governance(subject_id.clone()).await? {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                        } else {
                            self.subject_is_gov.insert(subject_id.clone(), false);
                            self.database.del_signatures(&subject_id, sn - 1)?;
                        }
                    }
                }
                transfer_request.subject_id.clone()
            }
            EventRequestType::EOL(eol_request) => {
                let subject_id = eol_request.subject_id.clone();
                // Aplicar event sourcing
                let mut subject =
                    self.database
                        .get_subject(&subject_id)
                        .map_err(|error| match error {
                            crate::DbError::EntryNotFound => {
                                LedgerError::SubjectNotFound(subject_id.to_str())
                            }
                            _ => LedgerError::DatabaseError(error),
                        })?;
                self.database.set_signatures(
                    &subject_id,
                    event.content.event_proposal.proposal.sn,
                    signatures,
                    validation_proof,
                )?;
                self.database.set_event(&subject_id, event.clone())?;
                self.set_finished_request(
                    &request_id,
                    event_request.clone(),
                    sn,
                    subject_id.clone(),
                )?;
                subject.eol_event();
                self.database.set_subject(&subject_id, subject)?;
                // Comprobar is_gov
                let is_gov = self.subject_is_gov.get(&subject_id);
                match is_gov {
                    Some(true) => {
                        // Enviar mensaje a gov de governance updated con el id y el sn
                        self.gov_api
                            .governance_updated(subject_id.clone(), sn)
                            .await?;
                    }
                    Some(false) => {
                        self.database.del_signatures(&subject_id, sn - 1)?;
                    }
                    None => {
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        if self.gov_api.is_governance(subject_id.clone()).await? {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // Enviar mensaje a gov de governance updated con el id y el sn
                            self.gov_api
                                .governance_updated(subject_id.clone(), sn)
                                .await?;
                        } else {
                            self.subject_is_gov.insert(subject_id.clone(), false);
                            self.database.del_signatures(&subject_id, sn - 1)?;
                        }
                    }
                }
                subject_id
            }
        };
        self.set_finished_request(
            &request_id,
            event_request.clone(),
            event.content.event_proposal.proposal.sn,
            subject_id.clone(),
        )?;
        // Actualizar Ledger State
        match self.ledger_state.entry(subject_id.clone()) {
            Entry::Occupied(mut ledger_state) => {
                let ledger_state = ledger_state.get_mut();
                let current_sn = ledger_state.current_sn.as_mut().unwrap();
                *current_sn = *current_sn + 1;
            }
            Entry::Vacant(entry) => {
                entry.insert(LedgerState {
                    current_sn: Some(0),
                    head: None,
                });
            }
        }
        // Enviar a Distribution info del nuevo event y que lo distribuya
        self.distribution_channel
            .tell(DistributionMessagesNew::SignaturesNeeded { subject_id, sn })
            .await?;
        Ok(())
    }

    pub async fn external_event(
        &mut self,
        event: Event,
        signatures: HashSet<Signature>,
        sender: KeyIdentifier,
        validation_proof: ValidationProof,
    ) -> Result<(), LedgerError> {
        // log::error!("External event: Event: {:?}", event);
        // Comprobar que no existe una request con el mismo hash
        let event_request = event.content.event_proposal.proposal.event_request.clone();
        let request_id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        match self.database.get_taple_request(&request_id) {
            Ok(_) => return Err(LedgerError::RepeatedRequestId(request_id.to_str())),
            Err(error) => match error {
                DbError::EntryNotFound => {}
                _ => return Err(LedgerError::DatabaseError(error)),
            },
        }
        // Comprobaciones criptográficas
        log::warn!("ANTES DE CHECK SIGNATURES");
        event.check_signatures()?;
        // Comprobar si es genesis o state
        match event
            .content
            .event_proposal
            .proposal
            .event_request
            .request
            .clone()
        {
            EventRequestType::Transfer(transfer_request) => {
                // Ledger state == None => No hay ni sujeto ni evento
                // CurrentSN == None => hay LCE pero no has recibido 0
                // CurrentSN == Some => Indica por dónde va el sujeto. Caché
                // HEAD == None => Estás al día
                // HEAD == SOME => HEAD INDICA EL LCE. NOS QUEDAMOS CON EL VALOR MENOR

                // No hace falta check_event porque no hay evaluación ni aprobación.
                // Comprobar Ledger State es None entonces es posible que

                // Se tiene que comprobar si la transferencia se está esperando.
                // Para ello, se consulta la base de datos y se comprueba el nuevo
                // propietario. Si somos nosotros, entonces tenemos la clave privada

                // Comprobaciones criptográficas
                let ledger_state = self.ledger_state.get(&transfer_request.subject_id);
                let metadata = validation_proof.get_metadata();
                match ledger_state {
                    Some(ledger_state) => {
                        match ledger_state.current_sn {
                            Some(current_sn) => {
                                if event.content.event_proposal.proposal.sn <= current_sn {
                                    return Err(LedgerError::EventAlreadyExists);
                                }
                            }
                            None => {
                                // Es LCE y tenemos otro LCE para un sujeto en el que no tenemos génesis ... TODO:
                                return Err(LedgerError::LCEBiggerSN);
                            }
                        }
                        let mut subject =
                            match self.database.get_subject(&transfer_request.subject_id) {
                                Ok(subject) => subject,
                                Err(crate::DbError::EntryNotFound) => {
                                    // Pedir génesis
                                    let msg = request_event(
                                        self.our_id.clone(),
                                        transfer_request.subject_id.clone(),
                                        0,
                                    );
                                    self.message_channel
                                        .tell(MessageTaskCommand::Request(
                                            None,
                                            msg,
                                            vec![sender],
                                            MessageConfig {
                                                timeout: 2000,
                                                replication_factor: 1.0,
                                            },
                                        ))
                                        .await?;
                                    return Err(LedgerError::SubjectNotFound(
                                        transfer_request.subject_id.to_str(),
                                    ));
                                }
                                Err(error) => {
                                    return Err(LedgerError::DatabaseError(error));
                                }
                            };
                        if !subject.active {
                            return Err(LedgerError::SubjectLifeEnd(subject.subject_id.to_str()));
                        }
                        let is_gov = self
                            .subject_is_gov
                            .get(&transfer_request.subject_id)
                            .unwrap();
                        if *is_gov {
                            // Comprobamos si existe head
                            if let Some(head) = ledger_state.head {
                                // Comprobamos si head == event.sn
                                if head == event.content.event_proposal.proposal.sn {
                                    // Pedimos el siguiente evento al que nosotros tenemos
                                    let msg = request_gov_event(
                                        self.our_id.clone(),
                                        subject.subject_id.clone(),
                                        subject.sn + 1,
                                    );
                                    self.message_channel
                                        .tell(MessageTaskCommand::Request(
                                            None,
                                            msg,
                                            vec![sender],
                                            MessageConfig {
                                                timeout: 2000,
                                                replication_factor: 1.0,
                                            },
                                        ))
                                        .await?;
                                    return Err(LedgerError::GovernanceLCE(
                                        transfer_request.subject_id.to_str(),
                                    ));
                                }
                            }
                        }
                        // Comprobar que las firmas son válidas y suficientes
                        // Si es el evento siguiente puedo obtener metadata de mi sistema, si es LCE lo tengo que obtener de la prueba de validación por si ha habido cambios de propietario u otros cambios
                        let mut witnesses = self.get_witnesses(metadata.clone()).await?;
                        if !witnesses.contains(&self.our_id) {
                            match self
                                .database
                                .get_preauthorized_subject_and_providers(&metadata.subject_id)
                            {
                                Ok(_) => {}
                                Err(error) => match error {
                                    crate::DbError::EntryNotFound => {
                                        return Err(LedgerError::WeAreNotWitnesses(
                                            transfer_request.subject_id.to_str(),
                                        ));
                                    }
                                    _ => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                },
                            }
                        }
                        self.check_transfer_event(event.clone())?;
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        let subject_id = transfer_request.subject_id.clone();
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        let prev_event_hash = if event.content.event_proposal.proposal.sn == 0 {
                            return Err(LedgerError::StateEventWithZeroSNDetected);
                        } else {
                            self.database
                                .get_event(
                                    &subject.subject_id,
                                    event.content.event_proposal.proposal.sn - 1,
                                )?
                                .signature
                                .content
                                .event_content_hash
                        };
                        let state_hash = subject.get_state_hash()?;
                        let validation_proof = ValidationProof::new_from_transfer_event(
                            &subject,
                            event.content.event_proposal.proposal.sn,
                            prev_event_hash,
                            event.signature.content.event_content_hash.clone(),
                            state_hash.clone(),
                            event.content.event_proposal.proposal.gov_version,
                            subject.owner.clone(),
                            transfer_request.public_key.clone(),
                        );
                        // let validation_proof = ValidationProof::new(
                        //     &subject,7 7
                        //     event.content.event_proposal.proposal.sn,
                        //     prev_event_hash,
                        //     event.signature.content.event_content_hash.clone(),
                        //     state_hash,
                        //     event.content.event_proposal.proposal.gov_version,
                        // );
                        let notary_hash = DigestIdentifier::from_serializable_borsh(
                            &validation_proof,
                        )
                        .map_err(|_| {
                            LedgerError::CryptoError(String::from(
                                "Error calculating the hash of the serializable",
                            ))
                        })?;
                        verify_signatures(&signatures, &signers, quorum, &notary_hash)?;
                        // Comprobar si es evento siguiente o LCE
                        if event.content.event_proposal.proposal.sn == subject.sn + 1
                            && ledger_state.head.is_none()
                        {
                            // Caso Evento Siguiente
                            // Comprobar ValidationProof
                            self.check_validation_proof(
                                &validation_proof,
                                &subject,
                                &event.signature.content.event_content_hash,
                                &state_hash,
                            )?;
                            let sn: u64 = event.content.event_proposal.proposal.sn;
                            // Comprobamos si estamos esperando la transferencia y si esta es a nosotros
                            let (keypair, to_delete) = if event
                                .content
                                .event_proposal
                                .proposal
                                .event_request
                                .signature
                                .content
                                .signer
                                == self.our_id
                            {
                                // TODO: ANALIZAR QUE DEBERÍAMOS HACER SI SE NOS TRANSFIERE Y NO LO QUEREMOS
                                // La transferencia es a nosotros
                                match self.database.get_keys(&transfer_request.public_key) {
                                    Ok(keypair) => (Some(keypair), true),
                                    Err(DbError::EntryNotFound) => {
                                        return Err(LedgerError::UnexpectedTransfer);
                                    }
                                    Err(error) => return Err(LedgerError::DatabaseError(error)),
                                }
                            } else {
                                (None, false)
                            };
                            subject.transfer_subject(
                                event
                                    .content
                                    .event_proposal
                                    .proposal
                                    .event_request
                                    .signature
                                    .content
                                    .signer
                                    .clone(),
                                transfer_request.public_key.clone(),
                                keypair,
                                event.content.event_proposal.proposal.sn,
                            );
                            self.database.set_signatures(
                                &transfer_request.subject_id,
                                sn,
                                signatures,
                                validation_proof.clone(),
                            )?;
                            self.database
                                .set_event(&transfer_request.subject_id, event)?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                sn,
                                subject_id.clone(),
                            )?;
                            self.database
                                .set_subject(&transfer_request.subject_id, subject)?;
                            if to_delete {
                                self.database.del_keys(&transfer_request.public_key)?;
                            }
                            if self.subject_is_gov.get(&subject_id).unwrap().to_owned() {
                                // Enviar mensaje a gov de governance updated con el id y el sn
                                let msg = request_gov_event(
                                    self.our_id.clone(),
                                    subject_id.clone(),
                                    sn + 1,
                                );
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                self.gov_api
                                    .governance_updated(subject_id.clone(), sn)
                                    .await?;
                            } else {
                                self.database
                                    .del_signatures(&transfer_request.subject_id, sn - 1)?;
                            }
                            self.ledger_state.insert(
                                transfer_request.subject_id.clone(),
                                LedgerState {
                                    current_sn: Some(sn),
                                    head: ledger_state.head,
                                },
                            );
                            // Mandar firma de testificacion a distribution manager o el evento en sí
                            self.distribution_channel
                                .tell(DistributionMessagesNew::SignaturesNeeded {
                                    subject_id: transfer_request.subject_id,
                                    sn,
                                })
                                .await?;
                        // } else if event.content.event_proposal.proposal.sn == subject.sn + 1 {
                        // Caso en el que el LCE es S + 1
                        // TODO:
                        } else if event.content.event_proposal.proposal.sn > subject.sn {
                            // Caso LCE
                            let is_gov = self.subject_is_gov.get(&subject_id).unwrap().to_owned();
                            if is_gov {
                                // No me valen los LCE de Gov
                                let msg = request_gov_event(
                                    self.our_id.clone(),
                                    subject_id,
                                    subject.sn + 1,
                                );
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                return Err(LedgerError::GovernanceLCE(
                                    transfer_request.subject_id.to_str(),
                                ));
                            }
                            // Comprobar que LCE es mayor y quedarnos con el mas peque si tenemos otro
                            let last_lce = match ledger_state.head {
                                Some(head) => {
                                    if event.content.event_proposal.proposal.sn > head {
                                        return Err(LedgerError::LCEBiggerSN);
                                    }
                                    Some(head)
                                }
                                None => {
                                    // Va a ser el nuevo LCE
                                    None
                                }
                            };
                            // Si hemos llegado aquí es porque va a ser nuevo LCE
                            let sn = event.content.event_proposal.proposal.sn;
                            self.database.set_signatures(
                                &transfer_request.subject_id,
                                sn,
                                signatures,
                                validation_proof.clone(),
                            )?;
                            self.database
                                .set_event(&transfer_request.subject_id, event)?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                sn,
                                subject_id.clone(),
                            )?;
                            self.database.set_lce_validation_proof(
                                &transfer_request.subject_id,
                                validation_proof,
                            )?;
                            if last_lce.is_some() {
                                let last_lce_sn = last_lce.unwrap();
                                self.database
                                    .del_signatures(&transfer_request.subject_id, last_lce_sn)?;
                                self.database
                                    .del_event(&transfer_request.subject_id, last_lce_sn)?;
                            } else {
                                // Borrar firmas de último evento validado
                                self.database
                                    .del_signatures(&transfer_request.subject_id, subject.sn)?;
                            }
                            self.ledger_state.insert(
                                transfer_request.subject_id.clone(),
                                LedgerState {
                                    current_sn: ledger_state.current_sn,
                                    head: Some(sn),
                                },
                            );
                            // Pedir evento siguiente a current_sn
                            witnesses.insert(subject.owner);
                            let msg =
                                request_event(self.our_id.clone(), transfer_request.subject_id, 0);
                            self.message_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    msg,
                                    witnesses.into_iter().collect(),
                                    MessageConfig {
                                        timeout: 2000,
                                        replication_factor: 0.8,
                                    },
                                ))
                                .await?;
                        } else {
                            // Caso evento repetido
                            return Err(LedgerError::EventAlreadyExists);
                        }
                    }
                    None => {
                        log::warn!("Pasa por NONE");
                        // Hacer comprobaciones con la ValidationProof
                        // Comprobar que las firmas son válidas y suficientes
                        let subject_id = transfer_request.subject_id.clone();
                        let metadata = validation_proof.get_metadata();
                        if &metadata.schema_id == "governance" {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // PEDIR GÉNESIS
                            let msg = request_gov_event(self.our_id.clone(), subject_id, 0);
                            self.message_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    msg,
                                    vec![sender],
                                    MessageConfig {
                                        timeout: 2000,
                                        replication_factor: 1.0,
                                    },
                                ))
                                .await?;
                            return Err(LedgerError::GovernanceLCE(
                                transfer_request.subject_id.to_str(),
                            ));
                        } else {
                            self.subject_is_gov.insert(subject_id.clone(), false);
                        }
                        let witnesses = self.get_witnesses(metadata.clone()).await?;
                        if !witnesses.contains(&self.our_id) {
                            match self
                                .database
                                .get_preauthorized_subject_and_providers(&metadata.subject_id)
                            {
                                Ok(_) => {}
                                Err(error) => match error {
                                    crate::DbError::EntryNotFound => {
                                        return Err(LedgerError::WeAreNotWitnesses(
                                            transfer_request.subject_id.to_str(),
                                        ));
                                    }
                                    _ => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                },
                            }
                        }
                        self.check_transfer_event(event.clone())?;
                        // self.check_event(event.clone(), metadata.clone()).await?;
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        let notary_hash = DigestIdentifier::from_serializable_borsh(
                            &validation_proof,
                        )
                        .map_err(|_| {
                            LedgerError::CryptoError(String::from(
                                "Error calculating the hash of the serializable",
                            ))
                        })?;
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        verify_signatures(&signatures, &signers, quorum, &notary_hash)?;
                        let sn = event.content.event_proposal.proposal.sn;
                        self.database.set_signatures(
                            &transfer_request.subject_id,
                            sn,
                            signatures,
                            validation_proof,
                        )?;
                        self.database
                            .set_event(&transfer_request.subject_id, event)?;
                        self.set_finished_request(
                            &request_id,
                            event_request.clone(),
                            sn,
                            subject_id.clone(),
                        )?;
                        self.ledger_state.insert(
                            transfer_request.subject_id.clone(),
                            LedgerState {
                                current_sn: None,
                                head: Some(sn),
                            },
                        );
                        // Pedir evento 0
                        let msg =
                            request_event(self.our_id.clone(), transfer_request.subject_id, 0);
                        self.message_channel
                            .tell(MessageTaskCommand::Request(
                                None,
                                msg,
                                vec![sender],
                                MessageConfig {
                                    timeout: 2000,
                                    replication_factor: 1.0,
                                },
                            ))
                            .await?;
                    }
                };
            }
            EventRequestType::Create(create_request) => {
                // Comprobar que evaluation es None
                if event.content.event_proposal.proposal.evaluation.is_some() {
                    return Err(LedgerError::ErrorParsingJsonString(
                        "Evaluation should be None in external genesis event".to_owned(),
                    ));
                }
                // Comprobaciones criptográficas
                let subject_id = generate_subject_id(
                    &create_request.namespace,
                    &create_request.schema_id,
                    create_request.public_key.to_str(),
                    create_request.governance_id.to_str(),
                    event.content.event_proposal.proposal.gov_version,
                )?;
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
                let our_gov_version = if &create_request.schema_id == "governance" {
                    0
                } else {
                    self.gov_api
                        .get_governance_version(
                            create_request.governance_id.clone(),
                            subject_id.clone(),
                        )
                        .await?
                };
                let metadata = Metadata {
                    namespace: create_request.namespace,
                    subject_id: subject_id.clone(),
                    governance_id: create_request.governance_id,
                    governance_version: our_gov_version,
                    schema_id: create_request.schema_id.clone(),
                };
                if &create_request.schema_id == "governance" {
                    match self
                        .database
                        .get_preauthorized_subject_and_providers(&subject_id)
                    {
                        Ok(_) => {}
                        Err(error) => match error {
                            crate::DbError::EntryNotFound => {
                                return Err(LedgerError::GovernanceNotPreauthorized(
                                    subject_id.to_str(),
                                ));
                            }
                            _ => {
                                return Err(LedgerError::DatabaseError(error));
                            }
                        },
                    }
                    // Enviar mensaje a gov de governance updated con el id y el sn
                    self.check_genesis(event, subject_id.clone()).await?;
                    self.gov_api
                        .governance_updated(subject_id.clone(), 0)
                        .await?;
                    self.subject_is_gov.insert(subject_id.clone(), true);
                } else {
                    // Comprobar que invoker tiene permisos de creación
                    let creators = self
                        .gov_api
                        .get_signers(metadata.clone(), ValidationStage::Create)
                        .await
                        .map_err(LedgerError::GovernanceError)?;
                    if !creators.contains(
                        &event
                            .content
                            .event_proposal
                            .proposal
                            .event_request
                            .signature
                            .content
                            .signer,
                    ) {
                        return Err(LedgerError::Unauthorized(format!(
                            "Creation unauthorized for KeyId: {}",
                            event
                                .content
                                .event_proposal
                                .proposal
                                .event_request
                                .signature
                                .content
                                .signer
                                .to_str()
                        )));
                    }
                    let witnesses = self.get_witnesses(metadata).await?;
                    if !witnesses.contains(&self.our_id) {
                        match self
                            .database
                            .get_preauthorized_subject_and_providers(&subject_id)
                        {
                            Ok(_) => {}
                            Err(error) => match error {
                                crate::DbError::EntryNotFound => {
                                    return Err(LedgerError::WeAreNotWitnesses(
                                        subject_id.to_str(),
                                    ));
                                }
                                _ => {
                                    return Err(LedgerError::DatabaseError(error));
                                }
                            },
                        }
                    }
                    self.check_genesis(event, subject_id.clone()).await?;
                    self.subject_is_gov.insert(subject_id.clone(), false);
                }
                match self.ledger_state.get_mut(&subject_id) {
                    Some(ledger_state) => {
                        ledger_state.current_sn = Some(0);
                    }
                    None => {
                        self.ledger_state.insert(
                            subject_id.clone(),
                            LedgerState {
                                current_sn: Some(0),
                                head: None,
                            },
                        );
                    }
                }
                // Enviar mensaje a distribution manager
                self.distribution_channel
                    .tell(DistributionMessagesNew::SignaturesNeeded { subject_id, sn: 0 })
                    .await?;
            }
            EventRequestType::Fact(state_request) => {
                // Comprobaciones criptográficas
                let ledger_state = self.ledger_state.get(&state_request.subject_id);
                let metadata = validation_proof.get_metadata();
                // Comprobar que invoker tiene permisos de invocación
                let invokers = self
                    .gov_api
                    .get_signers(metadata.clone(), ValidationStage::Invoke)
                    .await
                    .map_err(LedgerError::GovernanceError)?;
                if !invokers.contains(
                    &event
                        .content
                        .event_proposal
                        .proposal
                        .event_request
                        .signature
                        .content
                        .signer,
                ) {
                    return Err(LedgerError::Unauthorized(format!(
                        "Invokation unauthorized for KeyId: {}",
                        event
                            .content
                            .event_proposal
                            .proposal
                            .event_request
                            .signature
                            .content
                            .signer
                            .to_str()
                    )));
                }
                match ledger_state {
                    Some(ledger_state) => {
                        log::warn!("Pasa por SOME");
                        match ledger_state.current_sn {
                            Some(current_sn) => {
                                if event.content.event_proposal.proposal.sn <= current_sn {
                                    return Err(LedgerError::EventAlreadyExists);
                                }
                            }
                            None => {
                                // Es LCE y tenemos otro LCE para un sujeto en el que no tenemos génesis ... TODO:
                                return Err(LedgerError::LCEBiggerSN);
                            }
                        }
                        // Debemos comprobar si el sujeto es gobernanza
                        let mut subject = match self.database.get_subject(&state_request.subject_id)
                        {
                            Ok(subject) => subject,
                            Err(crate::DbError::EntryNotFound) => {
                                // Pedir génesis
                                let msg =
                                    request_event(self.our_id.clone(), state_request.subject_id, 0);
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                return Err(LedgerError::SubjectNotFound("aaa".into()));
                            }
                            Err(error) => {
                                return Err(LedgerError::DatabaseError(error));
                            }
                        };
                        if !subject.active {
                            return Err(LedgerError::SubjectLifeEnd(subject.subject_id.to_str()));
                        }
                        let is_gov = self.subject_is_gov.get(&state_request.subject_id).unwrap();
                        log::warn!("EL SUJETO ES  IS GOV: {}", is_gov);
                        if *is_gov {
                            // Al ser gov no tiene HEAD. Debemos comprobar si se trata del sn + 1
                            if event.content.event_proposal.proposal.sn > subject.sn + 1 {
                                // Pedimos el siguiente evento al que nosotros tenemos
                                let msg = request_gov_event(
                                    self.our_id.clone(),
                                    subject.subject_id.clone(),
                                    subject.sn + 1,
                                );
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                return Err(LedgerError::GovernanceLCE(
                                    state_request.subject_id.to_str(),
                                ));
                            }
                        }
                        // Comprobar que las firmas son válidas y suficientes
                        // Si es el evento siguiente puedo obtener metadata de mi sistema, si es LCE lo tengo que obtener de la prueba de validación por si ha habido cambios de propietario u otros cambios
                        log::warn!(
                            "ME LLEGA EL EVENTO CON SN {}",
                            event.content.event_proposal.proposal.sn
                        );
                        let mut witnesses = self.get_witnesses(metadata.clone()).await?;
                        log::warn!("GET TESTIGOS");
                        if !witnesses.contains(&self.our_id) {
                            match self
                                .database
                                .get_preauthorized_subject_and_providers(&metadata.subject_id)
                            {
                                Ok(_) => {}
                                Err(error) => match error {
                                    crate::DbError::EntryNotFound => {
                                        return Err(LedgerError::WeAreNotWitnesses(
                                            state_request.subject_id.to_str(),
                                        ));
                                    }
                                    _ => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                },
                            }
                        }
                        self.check_event(event.clone(), metadata.clone()).await?;
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        let subject_id = state_request.subject_id.clone();
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        let prev_event_hash = if event.content.event_proposal.proposal.sn == 0 {
                            DigestIdentifier::default()
                        } else {
                            self.database
                                .get_event(
                                    &subject.subject_id,
                                    event.content.event_proposal.proposal.sn - 1,
                                )?
                                .signature
                                .content
                                .event_content_hash
                        };
                        let state_hash = subject.state_hash_after_apply(
                            &event.content.event_proposal.proposal.json_patch,
                        )?;

                        let notary_hash = DigestIdentifier::from_serializable_borsh(
                            &validation_proof,
                        )
                        .map_err(|_| {
                            LedgerError::CryptoError(String::from(
                                "Error calculating the hash of the serializable",
                            ))
                        })?;

                        log::warn!("NOTARY HASH QUE ME LLEGA {}", notary_hash.to_str());
                        log::warn!("VALIDATION PROOF {:?}", validation_proof);
                        let notary_hash = DigestIdentifier::from_serializable_borsh(
                            &validation_proof,
                        )
                        .map_err(|_| {
                            LedgerError::CryptoError(String::from(
                                "Error calculating the hash of the serializable",
                            ))
                        })?;
                        log::warn!("SIGNATURES SIZE: {}", signatures.len());
                        log::warn!("SIGNERS SIZE {}", signers.len());
                        verify_signatures(&signatures, &signers, quorum, &notary_hash)?;
                        log::warn!("PASA POR VERIFY");
                        // Comprobar si es evento siguiente o LCE
                        log::warn!("EL LEDGER STATE ACTUAL ES {:?}", ledger_state);
                        if event.content.event_proposal.proposal.sn == subject.sn + 1
                            && ledger_state.head.is_none()
                        {
                            log::error!("EN EXTERNAL EVENT SE HACE E.SN == S.SN + 1");
                            // Caso Evento Siguiente
                            // Comprobar ValidationProof
                            self.check_validation_proof(
                                &validation_proof,
                                &subject,
                                &event.signature.content.event_content_hash,
                                &state_hash,
                            )?;
                            check_context(&event, &subject, metadata, subject.properties.clone())?;
                            let sn: u64 = event.content.event_proposal.proposal.sn;
                            let json_patch =
                                event.content.event_proposal.proposal.json_patch.as_str();
                            subject.update_subject(
                                json_patch,
                                event.content.event_proposal.proposal.sn,
                            )?;
                            self.database.set_signatures(
                                &state_request.subject_id,
                                sn,
                                signatures,
                                validation_proof,
                            )?;
                            self.database.set_event(&state_request.subject_id, event)?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                sn,
                                subject_id.clone(),
                            )?;
                            self.database
                                .set_subject(&state_request.subject_id, subject)?;
                            if self.subject_is_gov.get(&subject_id).unwrap().to_owned() {
                                // Enviar mensaje a gov de governance updated con el id y el sn
                                let msg = request_gov_event(
                                    self.our_id.clone(),
                                    subject_id.clone(),
                                    sn + 1,
                                );
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                self.gov_api
                                    .governance_updated(subject_id.clone(), sn)
                                    .await?;
                            } else {
                                self.database
                                    .del_signatures(&state_request.subject_id, sn - 1)?;
                            }
                            self.ledger_state.insert(
                                state_request.subject_id.clone(),
                                LedgerState {
                                    current_sn: Some(sn),
                                    head: ledger_state.head,
                                },
                            );
                            // Mandar firma de testificacion a distribution manager o el evento en sí
                            self.distribution_channel
                                .tell(DistributionMessagesNew::SignaturesNeeded {
                                    subject_id: state_request.subject_id,
                                    sn,
                                })
                                .await?;
                        // } else if event.content.event_proposal.proposal.sn == subject.sn + 1 {
                        // Caso en el que el LCE es S + 1
                        // TODO:
                        } else if event.content.event_proposal.proposal.sn > subject.sn {
                            log::error!("EN EXTERNAL EVENT SE HACE CASO LCE");
                            log::error!("SN EVENTO {}", event.content.event_proposal.proposal.sn);
                            log::error!("SN SUBJECT {}", subject.sn);
                            // Caso LCE
                            log::warn!("DEBERÍA EJECUTARSE ESTO");
                            let is_gov = self.subject_is_gov.get(&subject_id).unwrap().to_owned();
                            if is_gov {
                                // No me valen los LCE de Gov
                                let msg = request_gov_event(
                                    self.our_id.clone(),
                                    subject_id,
                                    subject.sn + 1,
                                );
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                return Err(LedgerError::GovernanceLCE(
                                    state_request.subject_id.to_str(),
                                ));
                            }
                            // Comprobar que LCE es mayor y quedarnos con el mas peque si tenemos otro
                            let last_lce = match ledger_state.head {
                                Some(head) => {
                                    if event.content.event_proposal.proposal.sn > head {
                                        return Err(LedgerError::LCEBiggerSN);
                                    }
                                    Some(head)
                                }
                                None => {
                                    // Va a ser el nuevo LCE
                                    None
                                }
                            };
                            // Si hemos llegado aquí es porque va a ser nuevo LCE
                            let sn = event.content.event_proposal.proposal.sn;
                            self.database.set_signatures(
                                &state_request.subject_id,
                                sn,
                                signatures,
                                validation_proof.clone(),
                            )?;
                            self.database.set_lce_validation_proof(
                                &state_request.subject_id,
                                validation_proof,
                            )?;
                            self.database.set_event(&state_request.subject_id, event)?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                sn,
                                subject_id.clone(),
                            )?;
                            if last_lce.is_some() {
                                let last_lce_sn = last_lce.unwrap();
                                self.database
                                    .del_signatures(&state_request.subject_id, last_lce_sn)?;
                                self.database
                                    .del_event(&state_request.subject_id, last_lce_sn)?;
                            } else {
                                // Borrar firmas de último evento validado
                                self.database
                                    .del_signatures(&state_request.subject_id, subject.sn)?;
                            }
                            self.ledger_state.insert(
                                state_request.subject_id.clone(),
                                LedgerState {
                                    current_sn: ledger_state.current_sn,
                                    head: Some(sn),
                                },
                            );
                            // Pedir evento siguiente a current_sn
                            witnesses.insert(subject.owner);
                            let msg =
                                request_event(self.our_id.clone(), state_request.subject_id, 0);
                            self.message_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    msg,
                                    witnesses.into_iter().collect(),
                                    MessageConfig {
                                        timeout: 2000,
                                        replication_factor: 0.8,
                                    },
                                ))
                                .await?;
                        } else {
                            // Caso evento repetido
                            return Err(LedgerError::EventAlreadyExists);
                        }
                    }
                    None => {
                        // Hacer comprobaciones con la ValidationProof
                        // Comprobar que las firmas son válidas y suficientes
                        let subject_id = state_request.subject_id.clone();
                        let metadata = validation_proof.get_metadata();
                        if &metadata.schema_id == "governance" {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // PEDIR GÉNESIS
                            let msg = request_gov_event(self.our_id.clone(), subject_id, 0);
                            self.message_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    msg,
                                    vec![sender],
                                    MessageConfig {
                                        timeout: 2000,
                                        replication_factor: 1.0,
                                    },
                                ))
                                .await?;
                            return Err(LedgerError::GovernanceLCE(
                                state_request.subject_id.to_str(),
                            ));
                        } else {
                            self.subject_is_gov.insert(subject_id.clone(), false);
                        }
                        let witnesses = self.get_witnesses(metadata.clone()).await?;
                        if !witnesses.contains(&self.our_id) {
                            match self
                                .database
                                .get_preauthorized_subject_and_providers(&metadata.subject_id)
                            {
                                Ok(_) => {}
                                Err(error) => match error {
                                    crate::DbError::EntryNotFound => {
                                        return Err(LedgerError::WeAreNotWitnesses(
                                            state_request.subject_id.to_str(),
                                        ));
                                    }
                                    _ => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                },
                            }
                        }
                        self.check_event(event.clone(), metadata.clone()).await?;
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        let notary_hash = DigestIdentifier::from_serializable_borsh(
                            &validation_proof,
                        )
                        .map_err(|_| {
                            LedgerError::CryptoError(String::from(
                                "Error calculating the hash of the serializable",
                            ))
                        })?;
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        verify_signatures(&signatures, &signers, quorum, &notary_hash)?;
                        let sn = event.content.event_proposal.proposal.sn;
                        self.database.set_signatures(
                            &state_request.subject_id,
                            sn,
                            signatures,
                            validation_proof,
                        )?;
                        self.database.set_event(&state_request.subject_id, event)?;
                        self.set_finished_request(
                            &request_id,
                            event_request.clone(),
                            sn,
                            subject_id.clone(),
                        )?;
                        self.ledger_state.insert(
                            state_request.subject_id.clone(),
                            LedgerState {
                                current_sn: None,
                                head: Some(sn),
                            },
                        );
                        // Pedir evento 0
                        let msg = request_event(self.our_id.clone(), state_request.subject_id, 0);
                        self.message_channel
                            .tell(MessageTaskCommand::Request(
                                None,
                                msg,
                                vec![sender],
                                MessageConfig {
                                    timeout: 2000,
                                    replication_factor: 1.0,
                                },
                            ))
                            .await?;
                    }
                };
            }
            EventRequestType::EOL(eol_request) => {
                let ledger_state = self.ledger_state.get(&eol_request.subject_id);
                let metadata = validation_proof.get_metadata();
                // Comprobar que invoker tiene permisos de invocación
                match ledger_state {
                    Some(ledger_state) => {
                        match ledger_state.current_sn {
                            Some(current_sn) => {
                                if event.content.event_proposal.proposal.sn <= current_sn {
                                    return Err(LedgerError::EventAlreadyExists);
                                }
                            }
                            None => {
                                // Es LCE y tenemos otro LCE para un sujeto en el que no tenemos génesis ... TODO:
                                return Err(LedgerError::LCEBiggerSN);
                            }
                        }
                        // Debemos comprobar si el sujeto es gobernanza
                        let mut subject = match self.database.get_subject(&eol_request.subject_id) {
                            Ok(subject) => subject,
                            Err(crate::DbError::EntryNotFound) => {
                                // Pedir génesis
                                let msg =
                                    request_event(self.our_id.clone(), eol_request.subject_id, 0);
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                return Err(LedgerError::SubjectNotFound("aaa".into()));
                            }
                            Err(error) => {
                                return Err(LedgerError::DatabaseError(error));
                            }
                        };
                        if !subject.active {
                            return Err(LedgerError::SubjectLifeEnd(subject.subject_id.to_str()));
                        }
                        if subject.creator
                            != event
                                .content
                                .event_proposal
                                .proposal
                                .event_request
                                .signature
                                .content
                                .signer
                        {
                            return Err(LedgerError::Unauthorized(
                                event
                                    .content
                                    .event_proposal
                                    .proposal
                                    .event_request
                                    .signature
                                    .content
                                    .signer
                                    .to_str(),
                            ));
                        }
                        let is_gov = self.subject_is_gov.get(&eol_request.subject_id).unwrap();
                        log::warn!("EL SUJETO ES  IS GOV: {}", is_gov);
                        if *is_gov {
                            // Al ser gov no tiene HEAD. Debemos comprobar si se trata del sn + 1
                            if event.content.event_proposal.proposal.sn > subject.sn + 1 {
                                // Pedimos el siguiente evento al que nosotros tenemos
                                let msg = request_gov_event(
                                    self.our_id.clone(),
                                    subject.subject_id.clone(),
                                    subject.sn + 1,
                                );
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                return Err(LedgerError::GovernanceLCE(
                                    eol_request.subject_id.to_str(),
                                ));
                            }
                        }
                        // Comprobar que las firmas son válidas y suficientes
                        // Si es el evento siguiente puedo obtener metadata de mi sistema, si es LCE lo tengo que obtener de la prueba de validación por si ha habido cambios de propietario u otros cambios
                        log::warn!(
                            "ME LLEGA EL EVENTO CON SN {}",
                            event.content.event_proposal.proposal.sn
                        );
                        let mut witnesses = self.get_witnesses(metadata.clone()).await?;
                        log::warn!("GET TESTIGOS");
                        if !witnesses.contains(&self.our_id) {
                            match self
                                .database
                                .get_preauthorized_subject_and_providers(&metadata.subject_id)
                            {
                                Ok(_) => {}
                                Err(error) => match error {
                                    crate::DbError::EntryNotFound => {
                                        return Err(LedgerError::WeAreNotWitnesses(
                                            eol_request.subject_id.to_str(),
                                        ));
                                    }
                                    _ => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                },
                            }
                        }
                        self.check_event(event.clone(), metadata.clone()).await?;
                        log::warn!("CHECK EVENT");
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        let subject_id = eol_request.subject_id.clone();
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        log::warn!("GET SIGNERS AND QUORUM");
                        let state_hash = subject.state_hash_after_apply(
                            &event.content.event_proposal.proposal.json_patch,
                        )?;

                        let notary_hash = DigestIdentifier::from_serializable_borsh(
                            &validation_proof,
                        )
                        .map_err(|_| {
                            LedgerError::CryptoError(String::from(
                                "Error calculating the hash of the serializable",
                            ))
                        })?;

                        log::warn!("NOTARY HASH QUE ME LLEGA {}", notary_hash.to_str());
                        log::warn!("VALIDATION PROOF {:?}", validation_proof);
                        let notary_hash = DigestIdentifier::from_serializable_borsh(
                            &validation_proof,
                        )
                        .map_err(|_| {
                            LedgerError::CryptoError(String::from(
                                "Error calculating the hash of the serializable",
                            ))
                        })?;
                        log::warn!("SIGNATURES SIZE: {}", signatures.len());
                        log::warn!("SIGNERS SIZE {}", signers.len());
                        verify_signatures(&signatures, &signers, quorum, &notary_hash)?;
                        log::warn!("PASA POR VERIFY");
                        // Comprobar si es evento siguiente o LCE
                        if event.content.event_proposal.proposal.sn == subject.sn + 1
                            && ledger_state.head.is_none()
                        {
                            // Caso Evento Siguiente
                            // Comprobar ValidationProof
                            self.check_validation_proof(
                                &validation_proof,
                                &subject,
                                &event.signature.content.event_content_hash,
                                &state_hash,
                            )?;
                            check_context(&event, &subject, metadata, subject.properties.clone())?;
                            let sn: u64 = event.content.event_proposal.proposal.sn;
                            subject.eol_event();
                            self.database.set_signatures(
                                &eol_request.subject_id,
                                sn,
                                signatures,
                                validation_proof,
                            )?;
                            self.database.set_event(&eol_request.subject_id, event)?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                sn,
                                subject_id.clone(),
                            )?;
                            self.database
                                .set_subject(&eol_request.subject_id, subject)?;
                            if self.subject_is_gov.get(&subject_id).unwrap().to_owned() {
                                // Enviar mensaje a gov de governance updated con el id y el sn
                                let msg = request_gov_event(
                                    self.our_id.clone(),
                                    subject_id.clone(),
                                    sn + 1,
                                );
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                self.gov_api
                                    .governance_updated(subject_id.clone(), sn)
                                    .await?;
                            } else {
                                self.database
                                    .del_signatures(&eol_request.subject_id, sn - 1)?;
                            }
                            self.ledger_state.insert(
                                eol_request.subject_id.clone(),
                                LedgerState {
                                    current_sn: Some(sn),
                                    head: ledger_state.head,
                                },
                            );
                            // Mandar firma de testificacion a distribution manager o el evento en sí
                            self.distribution_channel
                                .tell(DistributionMessagesNew::SignaturesNeeded {
                                    subject_id: eol_request.subject_id,
                                    sn,
                                })
                                .await?;
                        // } else if event.content.event_proposal.proposal.sn == subject.sn + 1 {
                        // Caso en el que el LCE es S + 1
                        // TODO:
                        } else if event.content.event_proposal.proposal.sn > subject.sn {
                            // Caso LCE
                            let is_gov = self.subject_is_gov.get(&subject_id).unwrap().to_owned();
                            if is_gov {
                                // No me valen los LCE de Gov
                                let msg = request_gov_event(
                                    self.our_id.clone(),
                                    subject_id,
                                    subject.sn + 1,
                                );
                                self.message_channel
                                    .tell(MessageTaskCommand::Request(
                                        None,
                                        msg,
                                        vec![sender],
                                        MessageConfig {
                                            timeout: 2000,
                                            replication_factor: 1.0,
                                        },
                                    ))
                                    .await?;
                                return Err(LedgerError::GovernanceLCE(
                                    eol_request.subject_id.to_str(),
                                ));
                            }
                            // Comprobar que LCE es mayor y quedarnos con el mas peque si tenemos otro
                            let last_lce = match ledger_state.head {
                                Some(head) => {
                                    if event.content.event_proposal.proposal.sn > head {
                                        return Err(LedgerError::LCEBiggerSN);
                                    } else {
                                        return Err(LedgerError::EOLWhenActiveLCE(
                                            eol_request.subject_id.to_str(),
                                        ));
                                    }
                                }
                                None => {
                                    // Va a ser el nuevo LCE
                                    None
                                }
                            };
                            // Si hemos llegado aquí es porque va a ser nuevo LCE
                            let sn = event.content.event_proposal.proposal.sn;
                            self.database.set_signatures(
                                &eol_request.subject_id,
                                sn,
                                signatures,
                                validation_proof.clone(),
                            )?;
                            self.database.set_lce_validation_proof(
                                &eol_request.subject_id,
                                validation_proof,
                            )?;
                            self.database.set_event(&eol_request.subject_id, event)?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                sn,
                                subject_id.clone(),
                            )?;
                            if last_lce.is_some() {
                                let last_lce_sn = last_lce.unwrap();
                                self.database
                                    .del_signatures(&eol_request.subject_id, last_lce_sn)?;
                                self.database
                                    .del_event(&eol_request.subject_id, last_lce_sn)?;
                            } else {
                                // Borrar firmas de último evento validado
                                self.database
                                    .del_signatures(&eol_request.subject_id, subject.sn)?;
                            }
                            self.ledger_state.insert(
                                eol_request.subject_id.clone(),
                                LedgerState {
                                    current_sn: ledger_state.current_sn,
                                    head: Some(sn),
                                },
                            );
                            // Pedir evento siguiente a current_sn
                            witnesses.insert(subject.owner);
                            let msg = request_event(self.our_id.clone(), eol_request.subject_id, 0);
                            self.message_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    msg,
                                    witnesses.into_iter().collect(),
                                    MessageConfig {
                                        timeout: 2000,
                                        replication_factor: 0.8,
                                    },
                                ))
                                .await?;
                        } else {
                            // Caso evento repetido
                            return Err(LedgerError::EventAlreadyExists);
                        }
                    }
                    None => {
                        // Es LCE
                        // Hacer comprobaciones con la ValidationProof
                        // Comprobar que las firmas son válidas y suficientes
                        let subject_id = eol_request.subject_id.clone();
                        let metadata = validation_proof.get_metadata();
                        if &metadata.schema_id == "governance" {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // PEDIR GÉNESIS
                            let msg = request_gov_event(self.our_id.clone(), subject_id, 0);
                            self.message_channel
                                .tell(MessageTaskCommand::Request(
                                    None,
                                    msg,
                                    vec![sender],
                                    MessageConfig {
                                        timeout: 2000,
                                        replication_factor: 1.0,
                                    },
                                ))
                                .await?;
                            return Err(LedgerError::GovernanceLCE(
                                eol_request.subject_id.to_str(),
                            ));
                        } else {
                            self.subject_is_gov.insert(subject_id.clone(), false);
                        }
                        let mut witnesses = self.get_witnesses(metadata.clone()).await?;
                        if !witnesses.contains(&self.our_id) {
                            match self
                                .database
                                .get_preauthorized_subject_and_providers(&metadata.subject_id)
                            {
                                Ok(_) => {}
                                Err(error) => match error {
                                    crate::DbError::EntryNotFound => {
                                        return Err(LedgerError::WeAreNotWitnesses(
                                            eol_request.subject_id.to_str(),
                                        ));
                                    }
                                    _ => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                },
                            }
                        }
                        self.check_event(event.clone(), metadata.clone()).await?;
                        // Si no está en el mapa, añadirlo y enviar mensaje a gov de subject updated con el id y el sn
                        let notary_hash = DigestIdentifier::from_serializable_borsh(
                            &validation_proof,
                        )
                        .map_err(|_| {
                            LedgerError::CryptoError(String::from(
                                "Error calculating the hash of the serializable",
                            ))
                        })?;
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        verify_signatures(&signatures, &signers, quorum, &notary_hash)?;
                        let sn = event.content.event_proposal.proposal.sn;
                        self.database.set_signatures(
                            &eol_request.subject_id,
                            sn,
                            signatures,
                            validation_proof,
                        )?;
                        let sn = event.content.event_proposal.proposal.sn;
                        self.database.set_event(&eol_request.subject_id, event)?;
                        self.set_finished_request(
                            &request_id,
                            event_request.clone(),
                            sn,
                            subject_id.clone(),
                        )?;
                        self.ledger_state.insert(
                            eol_request.subject_id.clone(),
                            LedgerState {
                                current_sn: None,
                                head: Some(sn),
                            },
                        );
                        // Pedir evento 0
                        let msg = request_event(self.our_id.clone(), eol_request.subject_id, 0);
                        self.message_channel
                            .tell(MessageTaskCommand::Request(
                                None,
                                msg,
                                vec![sender],
                                MessageConfig {
                                    timeout: 2000,
                                    replication_factor: 1.0,
                                },
                            ))
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn external_intermediate_event(&mut self, event: Event) -> Result<(), LedgerError> {
        let event_request = event.content.event_proposal.proposal.event_request.clone();
        let request_id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        match self.database.get_taple_request(&request_id) {
            Ok(_) => return Err(LedgerError::RepeatedRequestId(request_id.to_str())),
            Err(error) => match error {
                DbError::EntryNotFound => {}
                _ => return Err(LedgerError::DatabaseError(error)),
            },
        }
        // Comprobaciones criptográficas
        event.check_signatures()?;
        // Comprobar si es genesis o state
        let subject_id = match &event.content.event_proposal.proposal.event_request.request {
            EventRequestType::Create(create_request) => {
                // Comprobar si había un LCE previo o es genesis puro, si es genesis puro rechazar y que manden por la otra petición aunque sea con hashset de firmas vacío
                generate_subject_id(
                    &create_request.namespace,
                    &create_request.schema_id,
                    create_request.public_key.to_str(),
                    create_request.governance_id.to_str(),
                    event.content.event_proposal.proposal.gov_version,
                )?
            }
            EventRequestType::Fact(state_request) => state_request.subject_id.clone(),
            EventRequestType::Transfer(transfer_request) => transfer_request.subject_id.clone(),
            EventRequestType::EOL(eol_request) => {
                return Err(LedgerError::IntermediateEOL(
                    eol_request.subject_id.to_str(),
                ))
            }
        };
        let ledger_state = self.ledger_state.get(&subject_id);
        match ledger_state {
            Some(ledger_state) => {
                // Comprobar que tengo firmas de un evento mayor y que es el evento siguiente que necesito para este subject
                match ledger_state.head {
                    Some(head) => {
                        match ledger_state.current_sn {
                            Some(current_sn) => {
                                let subject = match self.database.get_subject(&subject_id) {
                                    Ok(subject) => subject,
                                    Err(crate::DbError::EntryNotFound) => {
                                        return Err(LedgerError::SubjectNotFound("".into()));
                                    }
                                    Err(error) => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                };
                                let metadata = Metadata {
                                    namespace: subject.namespace.clone(),
                                    subject_id: subject.subject_id.clone(),
                                    governance_id: subject.governance_id.clone(),
                                    governance_version: event
                                        .content
                                        .event_proposal
                                        .proposal
                                        .gov_version,
                                    schema_id: subject.schema_id.clone(),
                                };
                                // Comprobar que el evento es el siguiente
                                if event.content.event_proposal.proposal.sn == current_sn + 1 {
                                    // Comprobar que el evento es el que necesito
                                    match &event
                                        .content
                                        .event_proposal
                                        .proposal
                                        .event_request
                                        .request
                                    {
                                        EventRequestType::Create(_) => {
                                            return Err(LedgerError::UnexpectedCreateEvent)
                                        }
                                        EventRequestType::Fact(_) => {
                                            self.check_event(event.clone(), metadata.clone())
                                                .await?;
                                            check_context(
                                                &event,
                                                &subject,
                                                metadata.clone(),
                                                subject.properties.clone(),
                                            )?;
                                        }
                                        EventRequestType::Transfer(_) => {
                                            self.check_transfer_event(event.clone())?;
                                        }
                                        EventRequestType::EOL(_) => unreachable!(),
                                    }
                                    self.database.set_event(&subject_id, event.clone())?;
                                    self.set_finished_request(
                                        &request_id,
                                        event_request.clone(),
                                        event.content.event_proposal.proposal.sn,
                                        subject_id.clone(),
                                    )?;
                                    self.event_sourcing(event.clone())?;
                                    if head == current_sn + 2 {
                                        // Hacer event sourcing del LCE tambien y actualizar subject
                                        let head_event = self
                                            .database
                                            .get_event(&subject_id, head)
                                            .map_err(|error| match error {
                                                crate::database::Error::EntryNotFound => {
                                                    LedgerError::UnexpectEventMissingInEventSourcing
                                                }
                                                _ => LedgerError::DatabaseError(error),
                                            })?;
                                        // Comprobar ValidationProof
                                        let validation_proof =
                                            self.database.get_lce_validation_proof(&subject_id)?;
                                        let state_hash = subject.state_hash_after_apply(
                                            &head_event.content.event_proposal.proposal.json_patch,
                                        )?;
                                        // TODO: Si falla aquí inutilizamos sujeto???
                                        self.check_validation_proof(
                                            &validation_proof,
                                            &subject,
                                            &head_event.signature.content.event_content_hash,
                                            &state_hash,
                                        )?;
                                        self.event_sourcing(head_event)?;
                                        self.ledger_state.insert(
                                            subject_id.clone(),
                                            LedgerState {
                                                current_sn: Some(head),
                                                head: None,
                                            },
                                        );
                                        self.database.del_lce_validation_proof(&subject_id)?;
                                        // Se llega hasta el LCE con el event sourcing
                                        // Pedir firmas de testificación
                                        self.distribution_channel
                                            .tell(DistributionMessagesNew::SignaturesNeeded {
                                                subject_id: subject_id.clone(),
                                                sn: head,
                                            })
                                            .await?;
                                    } else {
                                        self.ledger_state.insert(
                                            subject_id.clone(),
                                            LedgerState {
                                                current_sn: Some(current_sn + 1),
                                                head: Some(head),
                                            },
                                        );
                                        let subject_owner =
                                            self.database.get_subject(&subject_id)?.owner;
                                        // No se llega hasta el LCE con el event sourcing
                                        // Pedir siguiente evento
                                        let mut witnesses =
                                            self.get_witnesses(metadata.clone()).await?;
                                        witnesses.insert(subject.owner);
                                        let msg = request_event(
                                            self.our_id.clone(),
                                            subject_id,
                                            current_sn + 2,
                                        );
                                        self.message_channel
                                            .tell(MessageTaskCommand::Request(
                                                None,
                                                msg,
                                                witnesses.into_iter().collect(),
                                                MessageConfig {
                                                    timeout: 2000,
                                                    replication_factor: 0.8,
                                                },
                                            ))
                                            .await?;
                                    }
                                    Ok(())
                                } else {
                                    // El evento no es el que necesito
                                    Err(LedgerError::EventNotNext)
                                }
                            }
                            None => {
                                // TODO: Comprobar antes del event sourcing si el LCE ES 1.

                                // El siguiente es el evento 0
                                if event.content.event_proposal.proposal.sn == 0 {
                                    // Comprobar que el evento 0 es el que necesito
                                    let metadata = self
                                        .check_genesis(event.clone(), subject_id.clone())
                                        .await?;
                                    // Check LCE validity. Ya no hace falta porque lo hacemos con la prueba de validación.
                                    // let lce = self.database.get_event(&subject_id, head)?;
                                    // match self.check_event(lce, metadata.clone()).await {
                                    //     Ok(_) => {}
                                    //     Err(error) => {
                                    //         log::error!("Error checking LCE: {}", error);
                                    //         // Borrar genesis y LCE
                                    //         self.database.del_event(&subject_id, 0)?;
                                    //         self.database.del_event(&subject_id, head)?;
                                    //         self.database.del_subject(&subject_id)?;
                                    //         self.database.del_signatures(&subject_id, head)?;
                                    //         return Err(LedgerError::InvalidLCEAfterGenesis(
                                    //             subject_id.to_str(),
                                    //         ));
                                    //     }
                                    // };
                                    if head == 1 {
                                        let subject = self.database.get_subject(&subject_id)?;
                                        let head_event = self
                                            .database
                                            .get_event(&subject_id, head)
                                            .map_err(|error| match error {
                                                crate::database::Error::EntryNotFound => {
                                                    LedgerError::UnexpectEventMissingInEventSourcing
                                                }
                                                _ => LedgerError::DatabaseError(error),
                                            })?;
                                        // Comprobar ValidationProof
                                        let validation_proof =
                                            self.database.get_lce_validation_proof(&subject_id)?;
                                        let state_hash = subject.state_hash_after_apply(
                                            &head_event.content.event_proposal.proposal.json_patch,
                                        )?;
                                        // TODO: Si falla aquí inutilizamos sujeto???
                                        self.check_validation_proof(
                                            &validation_proof,
                                            &subject,
                                            &head_event.signature.content.event_content_hash,
                                            &state_hash,
                                        )?;
                                        // Hacer event sourcing del evento 1 tambien y actualizar subject
                                        self.event_sourcing(head_event)?;
                                        self.ledger_state.insert(
                                            subject_id.clone(),
                                            LedgerState {
                                                current_sn: Some(1),
                                                head: None,
                                            },
                                        );
                                        self.database.del_lce_validation_proof(&subject_id)?;
                                        // Se llega hasta el LCE con el event sourcing
                                        // Pedir firmas de testificación
                                        self.distribution_channel
                                            .tell(DistributionMessagesNew::SignaturesNeeded {
                                                subject_id: subject_id.clone(),
                                                sn: 1,
                                            })
                                            .await?;
                                    } else {
                                        self.ledger_state.insert(
                                            subject_id.clone(),
                                            LedgerState {
                                                current_sn: Some(0),
                                                head: Some(head),
                                            },
                                        );
                                        // No se llega hasta el LCE con el event sourcing
                                        // Pedir siguiente evento
                                        let mut witnesses =
                                            self.get_witnesses(metadata.clone()).await?;
                                        // Añadir owner
                                        witnesses.insert(
                                            event
                                                .content
                                                .event_proposal
                                                .proposal
                                                .event_request
                                                .signature
                                                .content
                                                .signer,
                                        );
                                        let msg = request_event(self.our_id.clone(), subject_id, 1);
                                        self.message_channel
                                            .tell(MessageTaskCommand::Request(
                                                None,
                                                msg,
                                                witnesses.into_iter().collect(),
                                                MessageConfig {
                                                    timeout: 2000,
                                                    replication_factor: 0.8,
                                                },
                                            ))
                                            .await?;
                                    }
                                    Ok(())
                                } else {
                                    // El evento 0 no es el que necesito
                                    Err(LedgerError::UnsignedUnknowEvent)
                                }
                            }
                        }
                    }
                    None => Err(LedgerError::UnsignedUnknowEvent),
                }
            }
            None => Err(LedgerError::UnsignedUnknowEvent),
        }
    }

    pub async fn get_event(
        &self,
        who_asked: KeyIdentifier,
        subject_id: DigestIdentifier,
        sn: u64,
    ) -> Result<Event, LedgerError> {
        let event = self.database.get_event(&subject_id, sn)?;
        self.message_channel
            .tell(MessageTaskCommand::Request(
                None,
                TapleMessages::LedgerMessages(super::LedgerCommand::ExternalIntermediateEvent {
                    event: event.clone(),
                }),
                vec![who_asked],
                MessageConfig::direct_response(),
            ))
            .await?;
        Ok(event)
    }

    pub async fn get_next_gov(
        &self,
        who_asked: KeyIdentifier,
        subject_id: DigestIdentifier,
        sn: u64,
    ) -> Result<(Event, HashSet<Signature>), LedgerError> {
        log::warn!("GET NEXT GOV");
        log::info!("Getting NG: {}..............{}", subject_id.to_str(), sn);
        log::info!("Who Asked: {}", who_asked.to_str());
        let event = self.database.get_event(&subject_id, sn)?;
        let (signatures, validation_proof) = match self.database.get_signatures(&subject_id, sn) {
            Ok((s, validation_proof)) => (s, validation_proof),
            Err(error) => return Err(LedgerError::DatabaseError(error)),
        };
        self.message_channel
            .tell(MessageTaskCommand::Request(
                None,
                TapleMessages::LedgerMessages(super::LedgerCommand::ExternalEvent {
                    sender: self.our_id.clone(),
                    event: event.clone(),
                    signatures: signatures.clone(),
                    validation_proof,
                }),
                vec![who_asked],
                MessageConfig::direct_response(),
            ))
            .await?;
        Ok((event, signatures))
    }

    pub async fn get_lce(
        &self,
        who_asked: KeyIdentifier,
        subject_id: DigestIdentifier,
    ) -> Result<(Event, HashSet<Signature>), LedgerError> {
        log::info!("Getting LCE: {}", subject_id.to_str());
        log::info!("Who Asked: {}", who_asked.to_str());
        let subject = self.database.get_subject(&subject_id)?;
        let event = self.database.get_event(&subject_id, subject.sn)?;
        let (signatures, validation_proof) =
            match self.database.get_signatures(&subject_id, subject.sn) {
                Ok((s, validation_proof)) => (s, validation_proof),
                Err(error) => return Err(LedgerError::DatabaseError(error)),
            };

        self.message_channel
            .tell(MessageTaskCommand::Request(
                None,
                TapleMessages::LedgerMessages(super::LedgerCommand::ExternalEvent {
                    sender: self.our_id.clone(),
                    event: event.clone(),
                    signatures: signatures.clone(),
                    validation_proof,
                }),
                vec![who_asked],
                MessageConfig::direct_response(),
            ))
            .await?;
        Ok((event, signatures))
    }

    async fn get_witnesses(
        &self,
        metadata: Metadata,
    ) -> Result<HashSet<KeyIdentifier>, LedgerError> {
        let signers = self
            .gov_api
            .get_signers(metadata, ValidationStage::Witness)
            .await?;
        Ok(signers)
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

    fn check_validation_proof(
        &self,
        validation_proof: &ValidationProof,
        subject: &Subject,
        event_hash: &DigestIdentifier,
        state_hash: &DigestIdentifier,
    ) -> Result<(), LedgerError> {
        let hash_prev_event = match self.database.get_event(&subject.subject_id, subject.sn) {
            Ok(event) => event.signature.content.event_content_hash.clone(),
            Err(error) => match error {
                crate::DbError::EntryNotFound => {
                    if subject.sn == 0 {
                        DigestIdentifier::default()
                    } else {
                        return Err(LedgerError::DatabaseError(error));
                    }
                }
                _ => return Err(LedgerError::DatabaseError(error)),
            },
        };
        if subject.governance_id != validation_proof.governance_id {
            return Err(LedgerError::ValidationProofError(
                "Governance ID does not match".to_string(),
            ));
        } else if validation_proof.subject_id != subject.subject_id {
            return Err(LedgerError::ValidationProofError(
                "Subject ID does not match".to_string(),
            ));
        } else if validation_proof.sn != subject.sn + 1 {
            return Err(LedgerError::ValidationProofError(
                "Subject SN does not match".to_string(),
            ));
        } else if validation_proof.schema_id != subject.schema_id {
            return Err(LedgerError::ValidationProofError(
                "Schema ID does not match".to_string(),
            ));
        } else if validation_proof.namespace != subject.namespace {
            return Err(LedgerError::ValidationProofError(
                "Namespace does not match".to_string(),
            ));
        } else if validation_proof.prev_event_hash != hash_prev_event {
            return Err(LedgerError::ValidationProofError(
                "Hash Prev Event does not match".to_string(),
            ));
        } else if &validation_proof.event_hash != event_hash {
            return Err(LedgerError::ValidationProofError(
                "Hash Event does not match".to_string(),
            ));
        } else if state_hash != &validation_proof.state_hash {
            return Err(LedgerError::ValidationProofError(
                "Hash of State does not match".to_string(),
            ));
        } else if validation_proof.subject_public_key != subject.public_key {
            return Err(LedgerError::ValidationProofError(
                "Subject Public Key does not match".to_string(),
            ));
        } else if validation_proof.owner != subject.owner {
            return Err(LedgerError::ValidationProofError(
                "Owner does not match".to_string(),
            ));
        } else if validation_proof.creator != subject.creator {
            return Err(LedgerError::ValidationProofError(
                "Creator does not match".to_string(),
            ));
        }
        Ok(())
    }

    async fn check_genesis(
        &self,
        event: Event,
        subject_id: DigestIdentifier,
    ) -> Result<Metadata, LedgerError> {
        let EventRequestType::Create(create_request) = &event.content.event_proposal.proposal.event_request.request else {
            return Err(LedgerError::StateInGenesis);
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
        };
        // Ignoramos las firmas por ahora
        // Comprobar que el creador tiene permisos de creación
        if &create_request.schema_id != "governance" {
            let creation_roles = self
                .gov_api
                .get_signers(metadata.clone(), ValidationStage::Create)
                .await?;
            if !creation_roles.contains(&invoker) {
                return Err(LedgerError::Unauthorized("Crreator not allowed".into()));
            } // TODO: No estamos comprobando que pueda ser un external el que cree el subject y lo permitamos si tenia permisos.
        }
        // Crear sujeto y añadirlo a base de datos
        let init_state = self
            .gov_api
            .get_init_state(
                metadata.governance_id.clone(),
                metadata.schema_id.clone(),
                metadata.governance_version.clone(),
            )
            .await?;
        let init_state = serde_json::to_string(&init_state)
            .map_err(|_| LedgerError::ErrorParsingJsonString("Init state".to_owned()))?;
        let subject = Subject::from_genesis_event(event.clone(), init_state)?;
        self.database
            .set_governance_index(&subject_id, &subject.governance_id)?;
        let event_request = event.content.event_proposal.proposal.event_request.clone();
        let request_id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        let sn = event.content.event_proposal.proposal.sn;
        self.database.set_event(&subject_id, event)?;
        self.set_finished_request(&request_id, event_request.clone(), sn, subject_id.clone())?;
        self.database.set_subject(&subject_id, subject)?;
        Ok(metadata)
    }

    fn event_sourcing_eol(&self, event: Event) -> Result<(), LedgerError> {
        let subject_id = {
            match event.content.event_proposal.proposal.event_request.request {
                EventRequestType::EOL(eol_request) => eol_request.subject_id.clone(),
                _ => return Err(LedgerError::EventDoesNotFitHash),
            }
        };
        let sn = event.content.event_proposal.proposal.sn;
        let prev_event =
            self.database
                .get_event(&subject_id, sn - 1)
                .map_err(|error| match error {
                    crate::database::Error::EntryNotFound => {
                        LedgerError::UnexpectEventMissingInEventSourcing
                    }
                    _ => LedgerError::DatabaseError(error),
                })?;
        // Comprobar evento previo encaja
        if event.content.event_proposal.proposal.hash_prev_event
            != prev_event.signature.content.event_content_hash
        {
            return Err(LedgerError::EventDoesNotFitHash);
        }
        let mut subject = self.database.get_subject(&subject_id)?;
        subject.eol_event();
        self.database.set_subject(&subject_id, subject)?;
        Ok(())
    }

    fn event_sourcing_transfer(
        &self,
        subject_id: DigestIdentifier,
        sn: u64,
        owner: KeyIdentifier,
        public_key: KeyIdentifier,
    ) -> Result<(), LedgerError> {
        let prev_event =
            self.database
                .get_event(&subject_id, sn - 1)
                .map_err(|error| match error {
                    crate::database::Error::EntryNotFound => {
                        LedgerError::UnexpectEventMissingInEventSourcing
                    }
                    _ => LedgerError::DatabaseError(error),
                })?;
        let event = self
            .database
            .get_event(&subject_id, sn)
            .map_err(|error| match error {
                crate::database::Error::EntryNotFound => {
                    LedgerError::UnexpectEventMissingInEventSourcing
                }
                _ => LedgerError::DatabaseError(error),
            })?;
        // Comprobar evento previo encaja
        if event.content.event_proposal.proposal.hash_prev_event
            != prev_event.signature.content.event_content_hash
        {
            return Err(LedgerError::EventDoesNotFitHash);
        }
        let mut subject = self.database.get_subject(&subject_id)?;
        let (keypair, to_delete) = if event
            .content
            .event_proposal
            .proposal
            .event_request
            .signature
            .content
            .signer
            == self.our_id
        {
            // TODO: ANALIZAR QUE DEBERÍAMOS HACER SI SE NOS TRANSFIERE Y NO LO QUEREMOS
            // La transferencia es a nosotros
            match self.database.get_keys(&public_key) {
                Ok(keypair) => (Some(keypair), true),
                Err(DbError::EntryNotFound) => {
                    return Err(LedgerError::UnexpectedTransfer);
                }
                Err(error) => return Err(LedgerError::DatabaseError(error)),
            }
        } else {
            (None, false)
        };
        subject.transfer_subject(
            owner,
            public_key.clone(),
            keypair,
            event.content.event_proposal.proposal.sn,
        );
        self.database.set_subject(&subject_id, subject)?;
        if to_delete {
            self.database.del_keys(&public_key)?;
        }
        Ok(())
    }

    fn event_sourcing(&self, event: Event) -> Result<(), LedgerError> {
        match &event.content.event_proposal.proposal.event_request.request {
            EventRequestType::Transfer(transfer_request) => self.event_sourcing_transfer(
                transfer_request.subject_id.clone(),
                event.content.event_proposal.proposal.sn,
                event
                    .content
                    .event_proposal
                    .proposal
                    .event_request
                    .signature
                    .content
                    .signer
                    .clone(),
                transfer_request.public_key.clone(),
            ),
            EventRequestType::Create(_) => return Err(LedgerError::UnexpectedCreateEvent),
            EventRequestType::Fact(state_request) => self.event_sourcing_state(
                state_request.subject_id.clone(),
                event.content.event_proposal.proposal.sn,
                event,
            ),
            EventRequestType::EOL(eol_request) => self.event_sourcing_eol(event),
        }
    }

    fn event_sourcing_state(
        &self,
        subject_id: DigestIdentifier,
        sn: u64,
        event: Event,
    ) -> Result<(), LedgerError> {
        let prev_event =
            self.database
                .get_event(&subject_id, sn - 1)
                .map_err(|error| match error {
                    crate::database::Error::EntryNotFound => {
                        LedgerError::UnexpectEventMissingInEventSourcing
                    }
                    _ => LedgerError::DatabaseError(error),
                })?;
        // Comprobar evento previo encaja
        if event.content.event_proposal.proposal.hash_prev_event
            != prev_event.signature.content.event_content_hash
        {
            return Err(LedgerError::EventDoesNotFitHash);
        }
        let mut subject = self.database.get_subject(&subject_id)?;
        // let metadata = Metadata {
        //     namespace: subject.namespace.clone(),
        //     subject_id: subject.subject_id.clone(),
        //     governance_id: subject.governance_id.clone(),
        //     governance_version: event.content.event_proposal.proposal.gov_version,
        //     schema_id: subject.schema_id.clone(),
        //     owner: subject.owner.clone(),
        //     creator: subject.creator.clone(),
        // };
        // check_context(&event, metadata, subject.properties.clone())?;
        subject.update_subject(
            &event.content.event_proposal.proposal.json_patch,
            event.content.event_proposal.proposal.sn,
        )?;
        self.database.set_subject(&subject_id, subject)?;
        Ok(())
    }

    fn check_transfer_event(&self, event: Event) -> Result<(), LedgerError> {
        if event.content.event_proposal.proposal.evaluation.is_some() {
            return Err(LedgerError::EvaluationInTransferEvent);
        }
        if !event.content.approvals.is_empty() {
            return Err(LedgerError::ApprovalInTransferEvent);
        }
        Ok(())
    }

    async fn check_event(&self, event: Event, metadata: Metadata) -> Result<(), LedgerError> {
        // Comprobar que las firmas de evaluación y/o aprobación hacen quorum
        let (signers, quorum) = self
            .get_signers_and_quorum(metadata.clone(), ValidationStage::Evaluate)
            .await?;
        let evaluation_hash = DigestIdentifier::from_serializable_borsh(
            &event
                .content
                .event_proposal
                .proposal
                .evaluation
                .clone()
                .unwrap(),
        )
        .map_err(|_| {
            LedgerError::CryptoError(String::from(
                "Error calculating the hash of the serializable evaluation",
            ))
        })?;
        verify_signatures(
            &event.content.event_proposal.proposal.evaluation_signatures,
            &signers,
            quorum,
            &evaluation_hash,
        )?;
        if event
            .content
            .event_proposal
            .proposal
            .evaluation
            .clone()
            .unwrap()
            .approval_required
        {
            let (signers, quorum) = self
                .get_signers_and_quorum(metadata, ValidationStage::Approve)
                .await?;
            verify_approval_signatures(&event.content.approvals, &signers, quorum)?;
        }
        Ok(())
    }
}

fn check_context(
    event: &Event,
    subject: &Subject,
    metadata: Metadata,
    prev_properties: String,
) -> Result<(), LedgerError> {
    let event_preevaluation = EventPreEvaluation {
        event_request: event.content.event_proposal.proposal.event_request.clone(),
        context: Context {
            governance_id: metadata.governance_id,
            schema_id: metadata.schema_id,
            creator: subject.creator.clone(),
            owner: subject.owner.clone(),
            actual_state: prev_properties,
            namespace: metadata.namespace,
            governance_version: event.content.event_proposal.proposal.gov_version,
        },
        sn: event.content.event_proposal.proposal.sn,
    };
    let event_preevaluation_hash = DigestIdentifier::from_serializable_borsh(&event_preevaluation)
        .map_err(|_| {
            LedgerError::CryptoError(String::from(
                "Error calculating the hash of the event pre-evaluation",
            ))
        })?;
    if event_preevaluation_hash
        != event
            .content
            .event_proposal
            .proposal
            .evaluation
            .as_ref()
            .unwrap()
            .preevaluation_hash
    {
        return Err(LedgerError::CryptoError(String::from(
            "The hash of the event pre-evaluation calculed does not match with the one in the event",
        )));
    }
    Ok(())
}

fn verify_approval_signatures(
    approvals: &HashSet<Approval>,
    signers: &HashSet<KeyIdentifier>,
    quorum_size: u32,
) -> Result<(), LedgerError> {
    let mut actual_signers = HashSet::new();
    for approval in approvals.iter() {
        let approval_hash =
            DigestIdentifier::from_serializable_borsh(&approval.content).map_err(|_| {
                LedgerError::CryptoError(String::from(
                    "Error calculating the hash of the serializable approval",
                ))
            })?;
        let signature = approval.signature.clone();
        let signer = signature.content.signer.clone();
        if &signature.content.event_content_hash != &approval_hash {
            log::error!("Invalid Event Hash in Approval");
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
            "Not enough signatures Approval. Expected: {}, Actual: {}",
            quorum_size,
            actual_signers.len()
        );
        return Err(LedgerError::NotEnoughSignatures("Approval failed".into()));
    }
    Ok(())
}

fn verify_signatures(
    signatures: &HashSet<Signature>,
    signers: &HashSet<KeyIdentifier>,
    quorum_size: u32,
    event_hash: &DigestIdentifier,
) -> Result<(), LedgerError> {
    log::warn!("EL EVENT HASH ES {}", event_hash.to_str());
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
                "Signer {} in more than one validation/Evaluation signature",
                signer.to_str()
            );
            continue;
        }
    }
    if actual_signers.len() < quorum_size as usize {
        log::error!(
            "Not enough signatures Validation/Evaluation. Expected: {}, Actual: {}",
            quorum_size,
            actual_signers.len()
        );
        return Err(LedgerError::NotEnoughSignatures(event_hash.to_str()));
    }
    Ok(())
}
