use crate::commons::crypto::KeyGenerator;
use crate::commons::models::approval::ApprovalState;
use crate::commons::models::state::generate_subject_id;
use crate::crypto::Secp256k1KeyPair;
use crate::request::{RequestState, TapleRequest};
use crate::signature::Signed;
use crate::{
    commons::{
        channel::SenderEnd,
        models::{evaluation::SubjectContext, state::Subject, validation::ValidationProof},
    },
    crypto::{Ed25519KeyPair, KeyMaterial, KeyPair},
    database::{Error as DbError, DB},
    distribution::{error::DistributionErrorResponses, DistributionMessagesNew},
    governance::{stage::ValidationStage, GovernanceAPI, GovernanceInterface},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
    request::EventRequest,
    signature::Signature,
    utils::message::ledger::{request_event, request_gov_event},
    DatabaseCollection,
};
use crate::{
    ApprovalResponse, DigestDerivator, Event, KeyDerivator, Metadata, Notification, ValueWrapper,
};
use std::collections::{hash_map::Entry, HashMap, HashSet};

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
    notification_sender: tokio::sync::mpsc::Sender<Notification>,
    derivator: DigestDerivator,
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
        notification_sender: tokio::sync::mpsc::Sender<Notification>,
        derivator: DigestDerivator,
    ) -> Self {
        Self {
            gov_api,
            database,
            subject_is_gov: HashMap::new(),
            ledger_state: HashMap::new(),
            message_channel,
            distribution_channel,
            our_id,
            notification_sender,
            derivator
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
        // self.init_preautorized().await?;
        // Check if we have subjects halfway between current state and LCE
        // Update hashmaps
        let subjects = self.database.get_all_subjects();
        for subject in subjects.into_iter() {
            // Add it to is_gov
            if self
                .gov_api
                .is_governance(subject.subject_id.clone())
                .await?
            {
                self.subject_is_gov.insert(subject.subject_id.clone(), true);
                // Send message to gov of governance updated with id and sn
            } else {
                self.subject_is_gov
                    .insert(subject.subject_id.clone(), false);
            }
            // Update ledger_state for that subject
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
            if last_event.content.sn == pre_last_event.content.sn + 1 {
                if subject.sn != last_event.content.sn {
                    return Err(LedgerError::WrongSnInSubject(subject.subject_id.to_str()));
                }
                self.ledger_state.insert(
                    subject.subject_id,
                    LedgerState {
                        current_sn: Some(last_event.content.sn),
                        head: None,
                    },
                );
            } else {
                if subject.sn != pre_last_event.content.sn {
                    return Err(LedgerError::WrongSnInSubject(subject.subject_id.to_str()));
                }
                self.ledger_state.insert(
                    subject.subject_id,
                    LedgerState {
                        current_sn: Some(pre_last_event.content.sn),
                        head: Some(last_event.content.sn),
                    },
                );
            }
        }
        Ok(())
    }

    fn set_finished_request(
        &self,
        request_id: &DigestIdentifier,
        event_request: Signed<EventRequest>,
        sn: u64,
        subject_id: DigestIdentifier,
        success: bool,
    ) -> Result<(), LedgerError> {
        let mut taple_request: TapleRequest = event_request.clone().try_into()?;
        taple_request.sn = Some(sn);
        taple_request.subject_id = Some(subject_id.clone());
        taple_request.state = RequestState::Finished;
        taple_request.success = Some(success);
        self.database
            .set_taple_request(&request_id, &taple_request)?;
        Ok(())
    }

    pub async fn genesis(
        &mut self,
        event: Signed<Event>,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    ) -> Result<(), LedgerError> {
        let request_id = DigestIdentifier::generate_with_blake3(&event.content.event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        // Add to subject_is_gov if it is a governance and it is not
        let EventRequest::Create(create_request) = event.content.event_request.content.clone() else {
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
        let subject_keys = match self.database.get_keys(&create_request.public_key) {
            Ok(keys) => keys,
            Err(_) => {
                return Err(LedgerError::CryptoError(
                    "Error getting keys from database".to_owned(),
                ))
            }
        };
        // Create subject from genesis and event
        let subject = Subject::from_genesis_event(
            event.clone(),
            init_state,
            Some(subject_keys),
            self.derivator,
        )
        .map_err(LedgerError::SubjectError)?;
        let sn = event.content.sn;
        // Add subject and event to database
        let subject_id = subject.subject_id.clone();
        if &create_request.schema_id == "governance" {
            self.subject_is_gov.insert(subject_id.clone(), true);
            // Send message to gov of governance updated with id and sn
            self.gov_api
                .governance_updated(subject_id.clone(), sn)
                .await?;
        } else {
            self.subject_is_gov.insert(subject_id.clone(), false);
        }
        let ev_request = event.content.event_request.clone();
        self.database
            .set_governance_index(&subject_id, &subject.governance_id)?;
        self.database.set_subject(&subject_id, subject)?;
        self.database.set_signatures(
            &subject_id,
            event.content.sn,
            signatures,
            validation_proof, // Current Owner
        )?;
        self.database.set_event(&subject_id, event)?;
        self.set_finished_request(&request_id, ev_request, sn, subject_id.clone(), true)?;
        let _ = self
            .notification_sender
            .send(Notification::NewEvent {
                sn,
                subject_id: subject_id.to_str(),
            })
            .await
            .map_err(|_| LedgerError::NotificationChannelError);
        let _ = self
            .notification_sender
            .send(Notification::NewSubject {
                subject_id: subject_id.to_str(),
            })
            .await
            .map_err(|_| LedgerError::NotificationChannelError);
        // Upgrade Ledger State
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
        // Send subject_id and event in message to distribution manager
        self.distribution_channel
            .tell(DistributionMessagesNew::SignaturesNeeded { subject_id, sn: 0 })
            .await?;
        Ok(())
    }

    pub async fn generate_key(
        &self,
        derivator: KeyDerivator,
    ) -> Result<KeyIdentifier, LedgerError> {
        // Generate cryptographic material and save it in DB associated to subject_id
        // TODO: Make the choice of the dynamic MC. It is necessary first to make the change at state.rs level.
        let keys = match derivator {
            KeyDerivator::Ed25519 => KeyPair::Ed25519(Ed25519KeyPair::new()),
            KeyDerivator::Secp256k1 => KeyPair::Secp256k1(Secp256k1KeyPair::new()),
        };
        let public_key = KeyIdentifier::new(keys.get_key_derivator(), &keys.public_key_bytes());
        self.database.set_keys(&public_key, keys)?;
        Ok(public_key)
    }

    pub async fn event_validated(
        &mut self,
        event: Signed<Event>,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    ) -> Result<(), LedgerError> {
        let event_request = event.content.event_request.clone();
        let request_id = DigestIdentifier::generate_with_blake3(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        let sn = event.content.sn;
        let subject_id = match &event_request.content {
            EventRequest::Fact(state_request) => {
                let subject_id = state_request.subject_id.clone();
                // Apply event sourcing
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
                    event.content.sn,
                    signatures,
                    validation_proof, // Current Owner
                )?;
                let json_patch = event.content.patch.clone();
                if subject.sn != event.content.sn - 1 {
                    return Err(LedgerError::WrongSnInSubject(subject_id.to_str()));
                }
                if event.content.approved {
                    subject.update_subject(json_patch, event.content.sn)?;
                } else {
                    subject.sn = event.content.sn;
                }
                let _ = self
                    .notification_sender
                    .send(Notification::StateUpdated {
                        sn: event.content.sn,
                        subject_id: subject.subject_id.to_str(),
                    })
                    .await
                    .map_err(|_| LedgerError::NotificationChannelError);
                self.database.set_event(&subject_id, event.clone())?;
                self.set_finished_request(
                    &request_id,
                    event_request.clone(),
                    sn,
                    subject_id.clone(),
                    true,
                )?;
                let _ = self
                    .notification_sender
                    .send(Notification::NewEvent {
                        sn,
                        subject_id: subject_id.to_str(),
                    })
                    .await
                    .map_err(|_| LedgerError::NotificationChannelError);
                self.database.set_subject(&subject_id, subject)?;
                // Check is_gov
                let is_gov = self.subject_is_gov.get(&subject_id);
                match is_gov {
                    Some(true) => {
                        // Send message to gov of governance updated with id and sn
                        self.gov_api
                            .governance_updated(subject_id.clone(), sn)
                            .await?;
                    }
                    Some(false) => {
                        self.database.del_signatures(&subject_id, sn - 1)?;
                    }
                    None => {
                        // If not on the map, add it and send message to gov from subject updated with id and sn
                        if self.gov_api.is_governance(subject_id.clone()).await? {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // Send message to gov of governance updated with id and sn
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
            EventRequest::Create(_) => return Err(LedgerError::StateInGenesis),
            EventRequest::Transfer(transfer_request) => {
                let subject_id = transfer_request.subject_id.clone();
                // Apply event sourcing
                let mut subject =
                    self.database
                        .get_subject(&subject_id)
                        .map_err(|error| match error {
                            crate::DbError::EntryNotFound => {
                                LedgerError::SubjectNotFound(subject_id.to_str())
                            }
                            _ => LedgerError::DatabaseError(error),
                        })?;
                // Change subject's public key and remove cryptographic material
                subject.public_key = transfer_request.public_key.clone();
                subject.owner = event.content.event_request.signature.signer.clone();
                if subject.owner == self.our_id {
                    let keys = self.database.get_keys(&transfer_request.public_key)?;
                    subject.keys = Some(keys);
                } else {
                    subject.keys = None;
                }
                self.database.set_signatures(
                    &subject_id,
                    event.content.sn,
                    signatures,
                    validation_proof,
                )?;
                self.database.set_event(&subject_id, event.clone())?;
                self.set_finished_request(
                    &request_id,
                    event_request.clone(),
                    sn,
                    subject_id.clone(),
                    true,
                )?;
                let _ = self
                    .notification_sender
                    .send(Notification::NewEvent {
                        sn,
                        subject_id: subject_id.to_str(),
                    })
                    .await
                    .map_err(|_| LedgerError::NotificationChannelError);
                subject.sn = event.content.sn;
                self.database.set_subject(&subject_id, subject)?;
                let is_gov = self.subject_is_gov.get(&subject_id);
                match is_gov {
                    Some(true) => {}
                    Some(false) => {
                        self.database.del_signatures(&subject_id, sn - 1)?;
                    }
                    None => {
                        // If not on the map, add it and send message to gov from subject updated with id and sn
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
            EventRequest::EOL(eol_request) => {
                let subject_id = eol_request.subject_id.clone();
                // Apply event sourcing
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
                    event.content.sn,
                    signatures,
                    validation_proof,
                )?;
                self.database.set_event(&subject_id, event.clone())?;
                self.set_finished_request(
                    &request_id,
                    event_request.clone(),
                    sn,
                    subject_id.clone(),
                    true,
                )?;
                let _ = self
                    .notification_sender
                    .send(Notification::NewEvent {
                        sn,
                        subject_id: subject_id.to_str(),
                    })
                    .await
                    .map_err(|_| LedgerError::NotificationChannelError);
                subject.sn = sn;
                subject.eol_event();
                self.database.set_subject(&subject_id, subject)?;
                // Check is_gov
                let is_gov = self.subject_is_gov.get(&subject_id);
                match is_gov {
                    Some(true) => {
                        // Send message to gov of governance updated with id and sn
                        self.gov_api
                            .governance_updated(subject_id.clone(), sn)
                            .await?;
                    }
                    Some(false) => {
                        self.database.del_signatures(&subject_id, sn - 1)?;
                    }
                    None => {
                        // If not on the map, add it and send message to gov from subject updated with id and sn
                        if self.gov_api.is_governance(subject_id.clone()).await? {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // Send message to gov of governance updated with id and sn
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
            event.content.sn,
            subject_id.clone(),
            event.content.eval_success && event.content.approved,
        )?;
        // Upgrade Ledger State
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
        // Send to Distribution info of the new event and have them distribute it.
        self.distribution_channel
            .tell(DistributionMessagesNew::SignaturesNeeded { subject_id, sn })
            .await?;
        Ok(())
    }

    pub async fn external_event(
        &mut self,
        event: Signed<Event>,
        signatures: HashSet<Signature>,
        sender: KeyIdentifier,
        validation_proof: ValidationProof,
    ) -> Result<(), LedgerError> {
        // Check that no request with the same hash exists
        let event_request = event.content.event_request.clone();
        let request_id = DigestIdentifier::generate_with_blake3(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        let event_hash = DigestIdentifier::from_serializable_borsh(
            &event.content,
            validation_proof.event_hash.derivator.clone(),
        )
        .map_err(|_| LedgerError::CryptoError("Error generating event hash".to_owned()))?;
        match self.database.get_taple_request(&request_id) {
            Ok(_) => return Err(LedgerError::RepeatedRequestId(request_id.to_str())),
            Err(error) => match error {
                DbError::EntryNotFound => {}
                _ => return Err(LedgerError::DatabaseError(error)),
            },
        }
        // Cryptographic checks
        event.verify_signatures()?;
        // Check if it is genesis or state
        match event.content.event_request.content.clone() {
            EventRequest::Transfer(transfer_request) => {
                // Ledger state == None => There is neither subject nor event
                // CurrentSN == None => there is LCE but you have not received 0
                // CurrentSN == Some => Indicates where the subject goes. Cache
                // HEAD == None => You are up to date.
                // HEAD == SOME => HEAD INDICATES THE LCE. WE KEEP THE SMALLER VALUE

                // No check_event is needed because there is no evaluation or approval.
                // Check Ledger State is None so it is possible that

                // You have to check if the transfer is waiting.
                // This is done by querying the database and checking the new
                // owner. If it is us, then we have the private key.

                // Cryptographic checks
                let ledger_state = self.ledger_state.get(&transfer_request.subject_id);
                let metadata = validation_proof.get_metadata();
                match ledger_state {
                    Some(ledger_state) => {
                        match ledger_state.current_sn {
                            Some(current_sn) => {
                                if event.content.sn <= current_sn {
                                    return Err(LedgerError::EventAlreadyExists);
                                }
                            }
                            None => {
                                // It is LCE and we have another LCE for a subject in which we have no genesis ... TODO:
                                return Err(LedgerError::LCEBiggerSN);
                            }
                        }
                        let mut subject =
                            match self.database.get_subject(&transfer_request.subject_id) {
                                Ok(subject) => subject,
                                Err(crate::DbError::EntryNotFound) => {
                                    // Order genesis
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
                            // We check if head exists
                            if let Some(head) = ledger_state.head {
                                // We check if head == event.sn
                                if head == event.content.sn {
                                    // We ask for the following event to the one we have
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
                        // Check that the signatures are valid and sufficient
                        // If it is the following event I can get metadata from my system, if it is LCE I have to get it from the validation test in case there have been owner changes or other changes
                        let mut witnesses = self.get_witnesses(metadata.clone()).await?;
                        if !witnesses.contains(&self.our_id) {
                            match self
                                .database
                                .get_preauthorized_subject_and_providers(&metadata.subject_id)
                            {
                                Ok(_) => {}
                                Err(error) => match error {
                                    crate::DbError::EntryNotFound => {
                                        log::error!("{}", error);
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
                        // If not on the map, add it and send message to gov from subject updated with id and sn
                        let subject_id = transfer_request.subject_id.clone();
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        // let _prev_event_hash = if event.content.sn == 0 {
                        //     return Err(LedgerError::StateEventWithZeroSNDetected);
                        // } else {
                        //     let a = &self
                        //         .database
                        //         .get_event(&subject.subject_id, event.content.sn - 1);
                        //     if a.is_err() {
                        //         log::error!("SN {}", event.content.sn - 1);
                        //         log::error!("{:?}", a);
                        //     }
                        //     DigestIdentifier::from_serializable_borsh(
                        //         &self
                        //             .database
                        //             .get_event(&subject.subject_id, event.content.sn - 1)?
                        //             .content,
                        //     )
                        //     .map_err(|_| {
                        //         LedgerError::CryptoError(String::from(
                        //             "Error al calcular el hash del evento anterior",
                        //         ))
                        //     })?
                        // };
                        // let validation_proof_new = ValidationProof::new_from_transfer_event(
                        //     &subject,
                        //     event.content.sn,
                        //     prev_event_hash,
                        //     event_hash.clone(),
                        //     event.content.gov_version,
                        //     transfer_request.public_key.clone(),
                        // );
                        // // let validation_proof = ValidationProof::new(
                        // //     &subject,7 7
                        // //     event.content.sn,
                        // //     prev_event_hash,
                        // //     event.signature.content.event_content_hash.clone(),
                        // //     state_hash,
                        // //     event.content.gov_version,
                        // // );
                        // let notary_hash = DigestIdentifier::from_serializable_borsh(
                        //     &validation_proof,
                        // )
                        // .map_err(|_| {
                        //     LedgerError::CryptoError(String::from(
                        //         "Error calculating the hash of the serializable",
                        //     ))
                        // })?;
                        verify_signatures(&signatures, &signers, quorum, &validation_proof)?;
                        // Check if it is a next event or LCE
                        if event.content.sn == subject.sn + 1 && ledger_state.head.is_none() {
                            // Next Event Case
                            // Check ValidationProof
                            self.check_validation_proof(
                                &validation_proof,
                                &subject,
                                &event_hash,
                                &transfer_request.public_key,
                            )?;
                            let sn: u64 = event.content.sn;
                            // We check if we are waiting for the transfer and if it is to us.
                            let (keypair, to_delete) =
                                if event.content.event_request.signature.signer == self.our_id {
                                    // ALL: ANALYZE WHAT WE SHOULD DO IF IT IS TRANSFERRED TO US AND WE DON'T WANT IT
                                    // the transfer is to us
                                    match self.database.get_keys(&transfer_request.public_key) {
                                        Ok(keypair) => (Some(keypair), true),
                                        Err(DbError::EntryNotFound) => {
                                            return Err(LedgerError::UnexpectedTransfer);
                                        }
                                        Err(error) => {
                                            return Err(LedgerError::DatabaseError(error))
                                        }
                                    }
                                } else {
                                    (None, false)
                                };
                            subject.transfer_subject(
                                event.content.event_request.signature.signer.clone(),
                                transfer_request.public_key.clone(),
                                keypair,
                                event.content.sn,
                            );
                            self.database.set_signatures(
                                &transfer_request.subject_id,
                                sn,
                                signatures,
                                validation_proof.clone(),
                            )?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                event.content.sn,
                                subject_id.clone(),
                                true,
                            )?;
                            self.database
                                .set_event(&transfer_request.subject_id, event)?;
                            self.database
                                .set_subject(&transfer_request.subject_id, subject)?;
                            if to_delete {
                                self.database.del_keys(&transfer_request.public_key)?;
                            }
                            if self.subject_is_gov.get(&subject_id).unwrap().to_owned() {
                                // Send message to gov of governance updated with id and sn
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
                            // Send witnessing signature to distribution manager or the event itself
                            self.distribution_channel
                                .tell(DistributionMessagesNew::SignaturesNeeded {
                                    subject_id: transfer_request.subject_id,
                                    sn,
                                })
                                .await?;
                            // } else if event.content.sn == subject.sn + 1 {
                            // Case in which the LCE is S + 1
                            // TODO:
                        } else if event.content.sn > subject.sn {
                            // LCE Case
                            let is_gov = self.subject_is_gov.get(&subject_id).unwrap().to_owned();
                            if is_gov {
                                // Gov's LCEs do not work for me.
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
                            // Check which LCE is larger and keep the smaller one if we have another one.
                            let last_lce = match ledger_state.head {
                                Some(head) => {
                                    if event.content.sn > head {
                                        return Err(LedgerError::LCEBiggerSN);
                                    }
                                    Some(head)
                                }
                                None => {
                                    // It will be the new LCE
                                    None
                                }
                            };
                            // If we have arrived here it is because it is going to be a new LCE
                            let sn = event.content.sn;
                            self.database.set_signatures(
                                &transfer_request.subject_id,
                                sn,
                                signatures,
                                validation_proof.clone(),
                            )?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                event.content.sn,
                                subject_id.clone(),
                                event.content.eval_success && event.content.approved,
                            )?;
                            self.database
                                .set_event(&transfer_request.subject_id, event)?;
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
                                // Delete signatures of last validated event
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
                            // Request next event to current_sn
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
                            // Repeated event case
                            return Err(LedgerError::EventAlreadyExists);
                        }
                    }
                    None => {
                        // Make checks with the ValidationProof
                        // Check that the signatures are valid and sufficient
                        let subject_id = transfer_request.subject_id.clone();
                        let metadata = validation_proof.get_metadata();
                        if &metadata.schema_id == "governance" {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // ORDER GENESIS
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
                        // If not on the map, add it and send message to gov from subject updated with id and sn
                        // let notary_hash = DigestIdentifier::from_serializable_borsh(
                        //     &validation_proof,
                        // )
                        // .map_err(|_| {
                        //     LedgerError::CryptoError(String::from(
                        //         "Error calculating the hash of the serializable",
                        //     ))
                        // })?;
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        verify_signatures(&signatures, &signers, quorum, &validation_proof)?;
                        let sn = event.content.sn;
                        self.database.set_signatures(
                            &transfer_request.subject_id,
                            sn,
                            signatures,
                            validation_proof.clone(),
                        )?;
                        self.database.set_lce_validation_proof(
                            &transfer_request.subject_id,
                            validation_proof,
                        )?;
                        let success = event.content.eval_success && event.content.approved;
                        self.database
                            .set_event(&transfer_request.subject_id, event)?;
                        self.set_finished_request(
                            &request_id,
                            event_request.clone(),
                            sn,
                            subject_id.clone(),
                            success,
                        )?;
                        self.ledger_state.insert(
                            transfer_request.subject_id.clone(),
                            LedgerState {
                                current_sn: None,
                                head: Some(sn),
                            },
                        );
                        // Request event 0
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
            EventRequest::Create(create_request) => {
                // Check that evaluation is None
                if !event.content.eval_success {
                    return Err(LedgerError::ErrorParsingJsonString(
                        "Evaluation Success should be true in external genesis event".to_owned(),
                    ));
                }
                // Cryptographic checks
                let subject_id = generate_subject_id(
                    &create_request.namespace,
                    &create_request.schema_id,
                    create_request.public_key.to_str(),
                    create_request.governance_id.to_str(),
                    event.content.gov_version,
                    validation_proof.event_hash.derivator.clone(),
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
                    // Send message to gov of governance updated with id and sn
                    self.check_genesis(
                        event,
                        subject_id.clone(),
                        validation_proof.event_hash.derivator.clone(),
                    )
                    .await?;
                    self.gov_api
                        .governance_updated(subject_id.clone(), 0)
                        .await?;
                    self.subject_is_gov.insert(subject_id.clone(), true);
                } else {
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
                    self.check_genesis(
                        event,
                        subject_id.clone(),
                        validation_proof.event_hash.derivator.clone(),
                    )
                    .await?;
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
                // Send message to distribution manager
                self.distribution_channel
                    .tell(DistributionMessagesNew::SignaturesNeeded { subject_id, sn: 0 })
                    .await?;
            }
            EventRequest::Fact(state_request) => {
                let is_gov = self
                    .subject_is_gov
                    .get(&state_request.subject_id)
                    .unwrap_or(&false);
                // Cryptographic checks
                let ledger_state = self.ledger_state.get(&state_request.subject_id);
                let metadata = validation_proof.get_metadata();
                let sn = event.content.sn;
                match ledger_state {
                    Some(ledger_state) => {
                        match ledger_state.current_sn {
                            Some(current_sn) => {
                                if event.content.sn <= current_sn {
                                    return Err(LedgerError::EventAlreadyExists);
                                }
                            }
                            None => {
                                // It is LCE and we have another LCE for a subject in which we have no genesis .... TODO
                                return Err(LedgerError::LCEBiggerSN);
                            }
                        }
                        // We must check if the subject is governance
                        let mut subject = match self.database.get_subject(&state_request.subject_id)
                        {
                            Ok(subject) => subject,
                            Err(crate::DbError::EntryNotFound) => {
                                // ORDER GENESIS
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
                        let gov_id = subject.governance_id.clone();
                        let approval_request_hash = &event
                            .content
                            .get_approval_hash(gov_id, DigestDerivator::Blake3_256)
                            .map_err(|_| {
                                LedgerError::CryptoError(
                                    "Error generating approval request hash".to_owned(),
                                )
                            })?;
                        if !subject.active {
                            return Err(LedgerError::SubjectLifeEnd(subject.subject_id.to_str()));
                        }
                        if *is_gov {
                            // Since it is gov, it does not have HEAD. We must check if it is sn + 1
                            if event.content.sn > subject.sn + 1 {
                                // We ask for the following event to the one we have
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
                        // Check that invoker has invocation permissions
                        if subject.owner != event.content.event_request.signature.signer
                            && !self
                                .gov_api
                                .get_invoke_info(
                                    metadata.clone(),
                                    ValidationStage::Invoke,
                                    event.content.event_request.signature.signer.clone(),
                                )
                                .await
                                .map_err(LedgerError::GovernanceError)?
                        {
                            return Err(LedgerError::Unauthorized(format!(
                                "Invokation unauthorized for KeyId: {}",
                                event.content.event_request.signature.signer.to_str()
                            )));
                        }
                        // Check that the signatures are valid and sufficient
                        // If it is the following event I can get metadata from my system, if it is LCE I have to get it from the validation test in case there have been owner changes or other changes
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
                                            state_request.subject_id.to_str(),
                                        ));
                                    }
                                    _ => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                },
                            }
                        }
                        self.check_event(
                            event.clone(),
                            metadata.clone(),
                            subject.get_subject_context(
                                event.content.event_request.signature.signer.clone(),
                            ),
                        )
                        .await?;
                        // If not on the map, add it and send message to gov from subject updated with id and sn
                        let subject_id = state_request.subject_id.clone();
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        // let _prev_event_hash = if event.content.sn == 0 {
                        //     DigestIdentifier::default()
                        // } else {
                        //     DigestIdentifier::from_serializable_borsh(
                        //         &self
                        //             .database
                        //             .get_event(&subject.subject_id, event.content.sn - 1)?
                        //             .content,
                        //     )
                        //     .map_err(|_| {
                        //         LedgerError::CryptoError(String::from(
                        //             "Error calculating the hash of the serializable",
                        //         ))
                        //     })?
                        // };
                        verify_signatures(&signatures, &signers, quorum, &validation_proof)?;
                        // Check if it is a next event or LCE
                        if event.content.sn == subject.sn + 1 && ledger_state.head.is_none() {
                            // Next Event Case
                            // Check ValidationProof
                            self.check_validation_proof(
                                &validation_proof,
                                &subject,
                                &event_hash,
                                &subject.public_key,
                            )?;
                            let sn: u64 = event.content.sn;
                            let json_patch = event.content.patch.clone();
                            if event.content.approved {
                                subject.update_subject(json_patch, event.content.sn)?;
                            } else {
                                subject.sn = event.content.sn;
                            }
                            let _ = self
                                .notification_sender
                                .send(Notification::StateUpdated {
                                    sn: event.content.sn,
                                    subject_id: subject.subject_id.to_str(),
                                })
                                .await
                                .map_err(|_| LedgerError::NotificationChannelError);
                            self.database.set_signatures(
                                &state_request.subject_id,
                                sn,
                                signatures,
                                validation_proof,
                            )?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                event.content.sn,
                                subject_id.clone(),
                                event.content.eval_success && event.content.approved,
                            )?;
                            self.database.set_event(&state_request.subject_id, event)?;
                            let _ = self
                                .notification_sender
                                .send(Notification::NewEvent {
                                    sn,
                                    subject_id: subject_id.to_str(),
                                })
                                .await
                                .map_err(|_| LedgerError::NotificationChannelError);
                            self.database
                                .set_subject(&state_request.subject_id, subject)?;
                            if self.subject_is_gov.get(&subject_id).unwrap().to_owned() {
                                // Send message to gov of governance updated with id and sn
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
                            // Send witnessing signature to distribution manager or the event itself
                            self.distribution_channel
                                .tell(DistributionMessagesNew::SignaturesNeeded {
                                    subject_id: state_request.subject_id,
                                    sn,
                                })
                                .await?;
                        // } else if event.content.sn == subject.sn + 1 {
                        // Case in which the LCE is S + 1
                        // TODO:
                        } else if event.content.sn > subject.sn {
                            // LCE Case
                            let is_gov = self.subject_is_gov.get(&subject_id).unwrap().to_owned();
                            if is_gov {
                                // Gov's LCEs do not work for me.
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
                            // Check which LCE is larger and keep the smaller one if we have another one.
                            let last_lce = match ledger_state.head {
                                Some(head) => {
                                    if event.content.sn > head {
                                        return Err(LedgerError::LCEBiggerSN);
                                    }
                                    Some(head)
                                }
                                None => {
                                    // It will be the new LCE
                                    None
                                }
                            };
                            // If we have arrived here it is because it is going to be a new LCE
                            let sn = event.content.sn;
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
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                event.content.sn,
                                subject_id.clone(),
                                event.content.eval_success && event.content.approved,
                            )?;
                            self.database.set_event(&state_request.subject_id, event)?;
                            let _ = self
                                .notification_sender
                                .send(Notification::NewEvent {
                                    sn,
                                    subject_id: subject_id.to_str(),
                                })
                                .await
                                .map_err(|_| LedgerError::NotificationChannelError);
                            if last_lce.is_some() {
                                let last_lce_sn = last_lce.unwrap();
                                self.database
                                    .del_signatures(&state_request.subject_id, last_lce_sn)?;
                                self.database
                                    .del_event(&state_request.subject_id, last_lce_sn)?;
                            } else {
                                // Delete signatures of last validated event
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
                            // Request next event to current_sn
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
                            // Repeated event case
                            return Err(LedgerError::EventAlreadyExists);
                        }
                        match self.database.get_approval(&approval_request_hash) {
                            Ok(mut data) => {
                                if let ApprovalState::Pending = data.state {
                                    data.state = ApprovalState::Obsolete;
                                    self.database.set_approval(&approval_request_hash, data)?;
                                    let _ = self
                                        .notification_sender
                                        .send(Notification::ObsoletedApproval {
                                            id: approval_request_hash.to_str(),
                                            subject_id: subject_id.to_str(),
                                            sn,
                                        })
                                        .await
                                        .map_err(|_| LedgerError::NotificationChannelError);
                                }
                            }
                            Err(error) => match error {
                                DbError::EntryNotFound => {}
                                _ => {
                                    return Err(LedgerError::DatabaseError(error));
                                }
                            },
                        };
                    }
                    None => {
                        // Make checks with the ValidationProof
                        // Check that the signatures are valid and sufficient
                        let subject_id = state_request.subject_id.clone();
                        let metadata = validation_proof.get_metadata();
                        if &metadata.schema_id == "governance" {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // ORDER GENESIS
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
                        // NO LONGER POSSIBLE TO CHECK BECAUSE VALIDATION PROOF DOES NOT INDICATE WHO THE OWNER IS
                        // self.check_event(
                        //     event.clone(),
                        //     metadata.clone(),
                        //     subject.get_subject_context(
                        //         event.content.event_request.signature.signer.clone(),
                        //     ),
                        // )
                        // .await?;
                        // If not on the map, add it and send message to gov from subject updated with id and sn
                        // let notary_hash = DigestIdentifier::from_serializable_borsh(
                        //     &validation_proof,
                        // )
                        // .map_err(|_| {
                        //     LedgerError::CryptoError(String::from(
                        //         "Error calculating the hash of the serializable",
                        //     ))
                        // })?;
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        verify_signatures(&signatures, &signers, quorum, &validation_proof)?;
                        let sn = event.content.sn;
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
                        let success = event.content.eval_success && event.content.approved;
                        self.database.set_event(&state_request.subject_id, event)?;
                        self.set_finished_request(
                            &request_id,
                            event_request.clone(),
                            sn,
                            subject_id.clone(),
                            true,
                        )?;
                        let _ = self
                            .notification_sender
                            .send(Notification::NewEvent {
                                sn,
                                subject_id: subject_id.to_str(),
                            })
                            .await
                            .map_err(|_| LedgerError::NotificationChannelError);
                        self.set_finished_request(
                            &request_id,
                            event_request.clone(),
                            sn,
                            subject_id.clone(),
                            success,
                        )?;
                        self.ledger_state.insert(
                            state_request.subject_id.clone(),
                            LedgerState {
                                current_sn: None,
                                head: Some(sn),
                            },
                        );
                        // Request event 0
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
            EventRequest::EOL(eol_request) => {
                let ledger_state = self.ledger_state.get(&eol_request.subject_id);
                let metadata = validation_proof.get_metadata();
                // Check that invoker has invocation permissions
                match ledger_state {
                    Some(ledger_state) => {
                        match ledger_state.current_sn {
                            Some(current_sn) => {
                                if event.content.sn <= current_sn {
                                    return Err(LedgerError::EventAlreadyExists);
                                }
                            }
                            None => {
                                // It is LCE and we have another LCE for a subject in which we have no genesis .... TODO
                                return Err(LedgerError::LCEBiggerSN);
                            }
                        }
                        // We must check if the subject is governance
                        let mut subject = match self.database.get_subject(&eol_request.subject_id) {
                            Ok(subject) => subject,
                            Err(crate::DbError::EntryNotFound) => {
                                // ORDER GENESIS
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
                        if subject.creator != event.content.event_request.signature.signer {
                            return Err(LedgerError::Unauthorized(format!(
                                "Invokation unauthorized for KeyId: {}",
                                event.content.event_request.signature.signer.to_str()
                            )));
                        }
                        let is_gov = self.subject_is_gov.get(&eol_request.subject_id).unwrap();
                        if *is_gov {
                            // Since it is gov, it does not have HEAD. We must check if it is sn + 1
                            if event.content.sn > subject.sn + 1 {
                                // We ask for the following event to the one we have
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
                        // Check that the signatures are valid and sufficient
                        // If it is the following event I can get metadata from my system, if it is LCE I have to get it from the validation test in case there have been owner changes or other changes
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
                        self.check_event(
                            event.clone(),
                            metadata.clone(),
                            subject.get_subject_context(
                                event.content.event_request.signature.signer.clone(),
                            ),
                        )
                        .await?;
                        // If not on the map, add it and send message to gov from subject updated with id and sn
                        let subject_id = eol_request.subject_id.clone();
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        // let state_hash =
                        //     subject.state_hash_after_apply(event.content.patch.clone())?;
                        // let notary_hash = DigestIdentifier::from_serializable_borsh(
                        //     &validation_proof,
                        // )
                        // .map_err(|_| {
                        //     LedgerError::CryptoError(String::from(
                        //         "Error calculating the hash of the serializable",
                        //     ))
                        // })?;
                        verify_signatures(&signatures, &signers, quorum, &validation_proof)?;
                        // Check if it is a next event or LCE
                        if event.content.sn == subject.sn + 1 && ledger_state.head.is_none() {
                            // Next Event Case
                            // Check ValidationProof
                            self.check_validation_proof(
                                &validation_proof,
                                &subject,
                                &event_hash,
                                &subject.public_key,
                            )?;
                            let sn: u64 = event.content.sn;
                            subject.sn = sn;
                            subject.eol_event();
                            self.database.set_signatures(
                                &eol_request.subject_id,
                                sn,
                                signatures,
                                validation_proof,
                            )?;
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                event.content.sn,
                                subject_id.clone(),
                                true,
                            )?;
                            self.database.set_event(&eol_request.subject_id, event)?;
                            let _ = self
                                .notification_sender
                                .send(Notification::NewEvent {
                                    sn,
                                    subject_id: subject_id.to_str(),
                                })
                                .await
                                .map_err(|_| LedgerError::NotificationChannelError);
                            self.database
                                .set_subject(&eol_request.subject_id, subject)?;
                            if self.subject_is_gov.get(&subject_id).unwrap().to_owned() {
                                // Send message to gov of governance updated with id and sn
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
                            // Send witnessing signature to distribution manager or the event itself
                            self.distribution_channel
                                .tell(DistributionMessagesNew::SignaturesNeeded {
                                    subject_id: eol_request.subject_id,
                                    sn,
                                })
                                .await?;
                        // } else if event.content.sn == subject.sn + 1 {
                        // Case in which the LCE is S + 1
                        // TODO:
                        } else if event.content.sn > subject.sn {
                            // LCE Case
                            let is_gov = self.subject_is_gov.get(&subject_id).unwrap().to_owned();
                            if is_gov {
                                // Gov's LCEs do not work for me.
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
                            // Check which LCE is larger and keep the smaller one if we have another one.
                            let last_lce = match ledger_state.head {
                                Some(head) => {
                                    if event.content.sn > head {
                                        return Err(LedgerError::LCEBiggerSN);
                                    } else {
                                        return Err(LedgerError::EOLWhenActiveLCE(
                                            eol_request.subject_id.to_str(),
                                        ));
                                    }
                                }
                                None => {
                                    // It will be the new LCE
                                    None
                                }
                            };
                            // If we have arrived here it is because it is going to be a new LCE
                            let sn = event.content.sn;
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
                            self.set_finished_request(
                                &request_id,
                                event_request.clone(),
                                event.content.sn,
                                subject_id.clone(),
                                event.content.eval_success && event.content.approved,
                            )?;
                            self.database.set_event(&eol_request.subject_id, event)?;
                            let _ = self
                                .notification_sender
                                .send(Notification::NewEvent {
                                    sn,
                                    subject_id: subject_id.to_str(),
                                })
                                .await
                                .map_err(|_| LedgerError::NotificationChannelError);
                            if last_lce.is_some() {
                                let last_lce_sn = last_lce.unwrap();
                                self.database
                                    .del_signatures(&eol_request.subject_id, last_lce_sn)?;
                                self.database
                                    .del_event(&eol_request.subject_id, last_lce_sn)?;
                            } else {
                                // Delete signatures of last validated event
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
                            // Request next event to current_sn
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
                            // Repeated event case
                            return Err(LedgerError::EventAlreadyExists);
                        }
                    }
                    None => {
                        // It is LCE
                        // Make checks with the ValidationProof
                        // Check that the signatures are valid and sufficient
                        let subject_id = eol_request.subject_id.clone();
                        let metadata = validation_proof.get_metadata();
                        if &metadata.schema_id == "governance" {
                            self.subject_is_gov.insert(subject_id.clone(), true);
                            // ORDER GENESIS
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
                                            eol_request.subject_id.to_str(),
                                        ));
                                    }
                                    _ => {
                                        return Err(LedgerError::DatabaseError(error));
                                    }
                                },
                            }
                        }
                        // self.check_event(
                        //     event.clone(),
                        //     metadata.clone(),
                        //     subject.get_subject_context(
                        //         event.content.event_request.signature.signer.clone(),
                        //     ),
                        // )
                        // .await?;
                        // If not on the map, add it and send message to gov from subject updated with id and sn
                        // let notary_hash = DigestIdentifier::from_serializable_borsh(
                        //     &validation_proof,
                        // )
                        // .map_err(|_| {
                        //     LedgerError::CryptoError(String::from(
                        //         "Error calculating the hash of the serializable",
                        //     ))
                        // })?;
                        let (signers, quorum) = self
                            .get_signers_and_quorum(metadata.clone(), ValidationStage::Validate)
                            .await?;
                        verify_signatures(&signatures, &signers, quorum, &validation_proof)?;
                        let sn = event.content.sn;
                        self.database.set_signatures(
                            &eol_request.subject_id,
                            sn,
                            signatures,
                            validation_proof.clone(),
                        )?;
                        self.database
                            .set_lce_validation_proof(&eol_request.subject_id, validation_proof)?;
                        let sn = event.content.sn;
                        let success = event.content.eval_success && event.content.approved;
                        self.database.set_event(&eol_request.subject_id, event)?;
                        self.set_finished_request(
                            &request_id,
                            event_request.clone(),
                            sn,
                            subject_id.clone(),
                            success,
                        )?;
                        let _ = self
                            .notification_sender
                            .send(Notification::NewEvent {
                                sn,
                                subject_id: subject_id.to_str(),
                            })
                            .await
                            .map_err(|_| LedgerError::NotificationChannelError);
                        self.ledger_state.insert(
                            eol_request.subject_id.clone(),
                            LedgerState {
                                current_sn: None,
                                head: Some(sn),
                            },
                        );
                        // Request event 0
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

    pub async fn external_intermediate_event(
        &mut self,
        event: Signed<Event>,
    ) -> Result<(), LedgerError> {
        let event_request = event.content.event_request.clone();
        let request_id = DigestIdentifier::generate_with_blake3(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        match self.database.get_taple_request(&request_id) {
            Ok(_) => return Err(LedgerError::RepeatedRequestId(request_id.to_str())),
            Err(error) => match error {
                DbError::EntryNotFound => {}
                _ => return Err(LedgerError::DatabaseError(error)),
            },
        }
        // Cryptographic checks
        event.verify_signatures()?;
        // Check if it is genesis or state
        let subject_id = match &event.content.event_request.content {
            EventRequest::Create(create_request) => {
                // Check if there was a previous LCE or it is pure genesis, if it is pure genesis reject and send by the other petition even with empty signature hashset.
                generate_subject_id(
                    &create_request.namespace,
                    &create_request.schema_id,
                    create_request.public_key.to_str(),
                    create_request.governance_id.to_str(),
                    event.content.gov_version,
                    event.content.subject_id.derivator.clone(),
                )?
            }
            EventRequest::Fact(state_request) => state_request.subject_id.clone(),
            EventRequest::Transfer(transfer_request) => transfer_request.subject_id.clone(),
            EventRequest::EOL(eol_request) => {
                return Err(LedgerError::IntermediateEOL(
                    eol_request.subject_id.to_str(),
                ))
            }
        };
        if subject_id != event.content.subject_id {
            return Err(LedgerError::SubjectIdError);
        }
        let ledger_state = self.ledger_state.get(&subject_id);
        match ledger_state {
            Some(ledger_state) => {
                // Check that I have signatures from a major event and that is the next event I need for this subject
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
                                    governance_version: event.content.gov_version,
                                    schema_id: subject.schema_id.clone(),
                                };
                                if event.content.sn == current_sn + 1 {
                                    match &event.content.event_request.content {
                                        EventRequest::Create(_) => {
                                            return Err(LedgerError::UnexpectedCreateEvent)
                                        }
                                        EventRequest::Fact(_) => {
                                            self.check_event(
                                                event.clone(),
                                                metadata.clone(),
                                                subject.get_subject_context(
                                                    event
                                                        .content
                                                        .event_request
                                                        .signature
                                                        .signer
                                                        .clone(),
                                                ),
                                            )
                                            .await?;
                                        }
                                        EventRequest::Transfer(_) => {
                                            self.check_transfer_event(event.clone())?;
                                        }
                                        EventRequest::EOL(_) => unreachable!(),
                                    }
                                    self.database.set_event(&subject_id, event.clone())?;
                                    self.set_finished_request(
                                        &request_id,
                                        event_request.clone(),
                                        event.content.sn,
                                        subject_id.clone(),
                                        event.content.eval_success && event.content.approved,
                                    )?;
                                    let _ = self
                                        .notification_sender
                                        .send(Notification::NewEvent {
                                            sn: event.content.sn,
                                            subject_id: subject_id.to_str(),
                                        })
                                        .await
                                        .map_err(|_| LedgerError::NotificationChannelError);
                                    let approval_request_hash = &event
                                        .content
                                        .get_approval_hash(
                                            subject.governance_id.clone(),
                                            DigestDerivator::Blake3_256,
                                        )
                                        .map_err(|_| {
                                            LedgerError::CryptoError(
                                                "Error generating approval request hash".to_owned(),
                                            )
                                        })?;
                                    match self.database.get_approval(&approval_request_hash) {
                                        Ok(mut data) => {
                                            if let ApprovalState::Pending = data.state {
                                                data.state = ApprovalState::Obsolete;
                                                self.database
                                                    .set_approval(&approval_request_hash, data)?;
                                                let _ = self
                                                    .notification_sender
                                                    .send(Notification::ObsoletedApproval {
                                                        id: approval_request_hash.to_str(),
                                                        subject_id: subject_id.to_str(),
                                                        sn: event.content.sn,
                                                    })
                                                    .await
                                                    .map_err(|_| {
                                                        LedgerError::NotificationChannelError
                                                    });
                                            }
                                        }
                                        Err(error) => match error {
                                            DbError::EntryNotFound => {}
                                            _ => {
                                                return Err(LedgerError::DatabaseError(error));
                                            }
                                        },
                                    };
                                    let subject = self.event_sourcing(event.clone()).await?;
                                    if head == current_sn + 2 {
                                        // Do event sourcing of the LCE as well and update subject
                                        let head_event = self
                                            .database
                                            .get_event(&subject_id, head)
                                            .map_err(|error| match error {
                                                crate::database::Error::EntryNotFound => {
                                                    LedgerError::UnexpectEventMissingInEventSourcing
                                                }
                                                _ => LedgerError::DatabaseError(error),
                                            })?;
                                        // Check ValidationProof
                                        let validation_proof =
                                            self.database.get_lce_validation_proof(&subject_id)?;
                                        let public_key = if let EventRequest::Transfer(data) =
                                            &head_event.content.event_request.content
                                        {
                                            data.public_key.clone()
                                        } else {
                                            subject.public_key.clone()
                                        };
                                        let event_hash = DigestIdentifier::from_serializable_borsh(
                                            &head_event.content,
                                            validation_proof.event_hash.derivator.clone(),
                                        )
                                        .map_err(|_| {
                                            LedgerError::CryptoError(
                                                "Error generating event hash".to_owned(),
                                            )
                                        })?;
                                        self.check_validation_proof(
                                            &validation_proof,
                                            &subject,
                                            &event_hash,
                                            &public_key,
                                        )?;
                                        self.event_sourcing(head_event).await?;
                                        self.ledger_state.insert(
                                            subject_id.clone(),
                                            LedgerState {
                                                current_sn: Some(head),
                                                head: None,
                                            },
                                        );
                                        self.database.del_lce_validation_proof(&subject_id)?;

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
                                        let _subject_owner =
                                            self.database.get_subject(&subject_id)?.owner;
                                        // Event sourcing does not reach the ECL
                                        // Request next event
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
                                    // The event is not the one I need
                                    Err(LedgerError::EventNotNext)
                                }
                            }
                            None => {
                                // The following is event 0
                                if event.content.sn == 0 {
                                    // Check that event 0 is the one I need
                                    let metadata = self
                                        .check_genesis(
                                            event.clone(),
                                            subject_id.clone(),
                                            event.content.subject_id.derivator.clone(),
                                        )
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
                                        // Check ValidationProof
                                        let validation_proof =
                                            self.database.get_lce_validation_proof(&subject_id)?;
                                        let public_key = if let EventRequest::Transfer(data) =
                                            &head_event.content.event_request.content
                                        {
                                            data.public_key.clone()
                                        } else {
                                            subject.public_key.clone()
                                        };
                                        let event_hash = DigestIdentifier::from_serializable_borsh(
                                            &head_event.content,
                                            validation_proof.event_hash.derivator.clone(),
                                        )
                                        .map_err(|_| {
                                            LedgerError::CryptoError(
                                                "Error generating event hash".to_owned(),
                                            )
                                        })?;
                                        self.check_validation_proof(
                                            &validation_proof,
                                            &subject,
                                            &event_hash,
                                            &public_key,
                                        )?;
                                        // Hacer event sourcing del evento 1 tambien y actualizar subject
                                        self.event_sourcing(head_event).await?;
                                        self.ledger_state.insert(
                                            subject_id.clone(),
                                            LedgerState {
                                                current_sn: Some(1),
                                                head: None,
                                            },
                                        );
                                        self.database.del_lce_validation_proof(&subject_id)?;
                                        // The LCE is reached with event sourcing
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
                                        let mut witnesses =
                                            self.get_witnesses(metadata.clone()).await?;
                                        witnesses
                                            .insert(event.content.event_request.signature.signer);
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
    ) -> Result<Signed<Event>, LedgerError> {
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
    ) -> Result<(Signed<Event>, HashSet<Signature>), LedgerError> {
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
    ) -> Result<(Signed<Event>, HashSet<Signature>), LedgerError> {
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

    // TODO There is another one just like it in event manager, unify in one and put in utils
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
        public_key: &KeyIdentifier,
    ) -> Result<(), LedgerError> {
        let hash_prev_event = match self.database.get_event(&subject.subject_id, subject.sn) {
            Ok(event) => DigestIdentifier::from_serializable_borsh(
                &event.content,
                validation_proof.prev_event_hash.derivator.clone(),
            )
            .map_err(|_| {
                LedgerError::ValidationProofError("Error parsing prev event content".to_string())
            })?,
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
        } else if validation_proof.subject_public_key != *public_key {
            return Err(LedgerError::ValidationProofError(
                "Subject Public Key does not match".to_string(),
            ));
        } else if validation_proof.name != subject.name {
            return Err(LedgerError::ValidationProofError(
                "Subject Name does not match".to_string(),
            ));
        } else if validation_proof.genesis_governance_version != subject.genesis_gov_version {
            return Err(LedgerError::ValidationProofError(
                "Genesis gov versiob does not match".to_string(),
            ));
        }
        Ok(())
    }

    async fn check_genesis(
        &self,
        event: Signed<Event>,
        subject_id: DigestIdentifier,
        derivator: DigestDerivator,
    ) -> Result<Metadata, LedgerError> {
        let EventRequest::Create(create_request) = &event.content.event_request.content else {
            return Err(LedgerError::StateInGenesis);
        };

        let invoker = event.content.event_request.signature.signer.clone();
        let metadata = Metadata {
            namespace: create_request.namespace.clone(),
            subject_id: subject_id.clone(),
            governance_id: create_request.governance_id.clone(),
            governance_version: event.content.gov_version,
            schema_id: create_request.schema_id.clone(),
        };
        // We ignore signatures for now
        // Verify that the creator has creation permissions
        if &create_request.schema_id != "governance" {
            let creation_roles = self
                .gov_api
                .get_invoke_info(metadata.clone(), ValidationStage::Create, invoker)
                .await?;
            if !creation_roles {
                return Err(LedgerError::Unauthorized("Crreator not allowed".into()));
            } // TODO: We are not checking that it could be an external that creates the subject and we allow it if it had permissions.
        }
        // Create subject and add to database
        let init_state = self
            .gov_api
            .get_init_state(
                metadata.governance_id.clone(),
                metadata.schema_id.clone(),
                metadata.governance_version.clone(),
            )
            .await?;
        let subject = Subject::from_genesis_event(event.clone(), init_state, None, derivator)?;
        self.database
            .set_governance_index(&subject_id, &subject.governance_id)?;
        let event_request = event.content.event_request.clone();
        let request_id = DigestIdentifier::generate_with_blake3(&event_request)
            .map_err(|_| LedgerError::CryptoError("Error generating request hash".to_owned()))?;
        let sn = event.content.sn;
        let success = event.content.eval_success && event.content.approved;
        self.database.set_event(&subject_id, event)?;
        self.set_finished_request(
            &request_id,
            event_request.clone(),
            sn,
            subject_id.clone(),
            success,
        )?;
        let _ = self
            .notification_sender
            .send(Notification::NewEvent {
                sn,
                subject_id: subject_id.to_str(),
            })
            .await
            .map_err(|_| LedgerError::NotificationChannelError);
        self.database.set_subject(&subject_id, subject)?;
        let _ = self
            .notification_sender
            .send(Notification::NewSubject {
                subject_id: subject_id.to_str(),
            })
            .await
            .map_err(|_| LedgerError::NotificationChannelError);
        Ok(metadata)
    }

    fn event_sourcing_eol(&self, event: Signed<Event>) -> Result<Subject, LedgerError> {
        let subject_id = {
            match event.content.event_request.content {
                EventRequest::EOL(eol_request) => eol_request.subject_id.clone(),
                _ => return Err(LedgerError::EventDoesNotFitHash),
            }
        };
        let sn = event.content.sn;
        let prev_event_hash = DigestIdentifier::from_serializable_borsh(
            &self
                .database
                .get_event(&subject_id, sn - 1)
                .map_err(|error| match error {
                    crate::database::Error::EntryNotFound => {
                        LedgerError::UnexpectEventMissingInEventSourcing
                    }
                    _ => LedgerError::DatabaseError(error),
                })?
                .content,
            event.content.hash_prev_event.derivator.clone(),
        )
        .map_err(|_| LedgerError::CryptoError("Error generating hash".to_owned()))?;
        // Check previous event fits
        if event.content.hash_prev_event != prev_event_hash {
            return Err(LedgerError::EventDoesNotFitHash);
        }
        let mut subject = self.database.get_subject(&subject_id)?;
        subject.eol_event();
        self.database.set_subject(&subject_id, subject.clone())?;
        Ok(subject)
    }

    fn event_sourcing_transfer(
        &self,
        subject_id: DigestIdentifier,
        sn: u64,
        owner: KeyIdentifier,
        public_key: KeyIdentifier,
    ) -> Result<Subject, LedgerError> {
        let event = self
            .database
            .get_event(&subject_id, sn)
            .map_err(|error| match error {
                crate::database::Error::EntryNotFound => {
                    LedgerError::UnexpectEventMissingInEventSourcing
                }
                _ => LedgerError::DatabaseError(error),
            })?;
        let prev_event_hash = DigestIdentifier::from_serializable_borsh(
            &self
                .database
                .get_event(&subject_id, sn - 1)
                .map_err(|error| match error {
                    crate::database::Error::EntryNotFound => {
                        LedgerError::UnexpectEventMissingInEventSourcing
                    }
                    _ => LedgerError::DatabaseError(error),
                })?
                .content,
                event.content.hash_prev_event.derivator.clone()
        )
        .map_err(|_| LedgerError::CryptoError("Error generating hash".to_owned()))?;
        // Check previous event fits
        if event.content.hash_prev_event != prev_event_hash {
            return Err(LedgerError::EventDoesNotFitHash);
        }
        let mut subject = self.database.get_subject(&subject_id)?;
        let (keypair, to_delete) = if event.content.event_request.signature.signer == self.our_id {
            // TODO: ANALYZE WHAT WE SHOULD DO IF WE ARE TRANSFERRED AND WE DO NOT WANT IT
            // The transfer is to us
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
        subject.transfer_subject(owner, public_key.clone(), keypair, event.content.sn);
        self.database.set_subject(&subject_id, subject.clone())?;
        if to_delete {
            self.database.del_keys(&public_key)?;
        }
        Ok(subject)
    }

    async fn event_sourcing(&self, event: Signed<Event>) -> Result<Subject, LedgerError> {
        if !event.content.approved {
            self.event_sourcing_rejected_event(event).await
        } else {
            match &event.content.event_request.content {
                EventRequest::Transfer(transfer_request) => self.event_sourcing_transfer(
                    transfer_request.subject_id.clone(),
                    event.content.sn,
                    event.content.event_request.signature.signer.clone(),
                    transfer_request.public_key.clone(),
                ),
                EventRequest::Create(_) => Err(LedgerError::UnexpectedCreateEvent),
                EventRequest::Fact(state_request) => {
                    self.event_sourcing_state(
                        state_request.subject_id.clone(),
                        event.content.sn,
                        event,
                    )
                    .await
                }
                EventRequest::EOL(_) => self.event_sourcing_eol(event),
            }
        }
    }

    async fn event_sourcing_rejected_event(
        &self,
        event: Signed<Event>,
    ) -> Result<Subject, LedgerError> {
        let prev_event_hash = DigestIdentifier::from_serializable_borsh(
            &self
                .database
                .get_event(&event.content.subject_id, event.content.sn - 1)
                .map_err(|error| match error {
                    crate::database::Error::EntryNotFound => {
                        LedgerError::UnexpectEventMissingInEventSourcing
                    }
                    _ => LedgerError::DatabaseError(error),
                })?
                .content,
                event.content.hash_prev_event.derivator.clone()
        )
        .map_err(|_| LedgerError::CryptoError("Error generating hash".to_owned()))?;
        // Check previous event fits
        if event.content.hash_prev_event != prev_event_hash {
            return Err(LedgerError::EventDoesNotFitHash);
        }
        let mut subject = self.database.get_subject(&event.content.subject_id)?;
        if event.content.sn != subject.sn + 1 {
            return Err(LedgerError::EventNotNext);
        }
        subject.sn = event.content.sn;
        let _ = self
            .notification_sender
            .send(Notification::StateUpdated {
                sn: event.content.sn,
                subject_id: subject.subject_id.to_str(),
            })
            .await
            .map_err(|_| LedgerError::NotificationChannelError);
        self.database
            .set_subject(&event.content.subject_id, subject.clone())?;
        Ok(subject)
    }

    async fn event_sourcing_state(
        &self,
        subject_id: DigestIdentifier,
        sn: u64,
        event: Signed<Event>,
    ) -> Result<Subject, LedgerError> {
        let prev_event_hash = DigestIdentifier::from_serializable_borsh(
            &self
                .database
                .get_event(&subject_id, sn - 1)
                .map_err(|error| match error {
                    crate::database::Error::EntryNotFound => {
                        LedgerError::UnexpectEventMissingInEventSourcing
                    }
                    _ => LedgerError::DatabaseError(error),
                })?
                .content,
                event.content.hash_prev_event.derivator.clone()
        )
        .map_err(|_| LedgerError::CryptoError("Error generating hash".to_owned()))?;
        // Check previous event fits
        if event.content.hash_prev_event != prev_event_hash {
            return Err(LedgerError::EventDoesNotFitHash);
        }
        let mut subject = self.database.get_subject(&subject_id)?;
        // let metadata = Metadata {
        //     namespace: subject.namespace.clone(),
        //     subject_id: subject.subject_id.clone(),
        //     governance_id: subject.governance_id.clone(),
        //     governance_version: event.content.gov_version,
        //     schema_id: subject.schema_id.clone(),
        //     owner: subject.owner.clone(),
        //     creator: subject.creator.clone(),
        // };
        // check_context(&event, metadata, subject.properties.clone())?;
        subject.update_subject(event.content.patch, event.content.sn)?;
        let _ = self
            .notification_sender
            .send(Notification::StateUpdated {
                sn: event.content.sn,
                subject_id: subject.subject_id.to_str(),
            })
            .await
            .map_err(|_| LedgerError::NotificationChannelError);
        self.database.set_subject(&subject_id, subject.clone())?;
        Ok(subject)
    }

    fn check_transfer_event(&self, event: Signed<Event>) -> Result<(), LedgerError> {
        if !event.content.eval_success
            || event.content.patch
                != ValueWrapper(serde_json::from_str("[]").map_err(|_| {
                    LedgerError::CryptoError("Error parsing empty json".to_string())
                })?)
        {
            return Err(LedgerError::EvaluationInTransferEvent);
        }
        if event.content.appr_required || !event.content.approved {
            return Err(LedgerError::ApprovalInTransferEvent);
        }
        Ok(())
    }

    async fn check_event(
        &self,
        event: Signed<Event>,
        metadata: Metadata,
        subject_context: SubjectContext,
    ) -> Result<(), LedgerError> {
        // Verify that the evaluation and/or approval signatures make quorum
        let (signers_eval, quorum_eval) = self
            .get_signers_and_quorum(metadata.clone(), ValidationStage::Evaluate)
            .await?;
        let quorum_neg_eval = (signers_eval.len() as u32 - quorum_eval) + 1;
        let (signers, quorum) = self
            .get_signers_and_quorum(metadata, ValidationStage::Approve)
            .await?;
        let quorum_neg = (signers.len() as u32 - quorum) + 1;
        event.verify_eval_appr(
            subject_context,
            (&signers_eval, quorum_eval, quorum_neg_eval),
            (&signers, quorum, quorum_neg),
        )?;
        Ok(())
    }
}

#[allow(dead_code)]
fn verify_approval_signatures(
    approvals: &HashSet<Signed<ApprovalResponse>>,
    signers: &HashSet<KeyIdentifier>,
    quorum_size: u32,
    event_proposal_hash: DigestIdentifier,
) -> Result<(), LedgerError> {
    let mut actual_signers = HashSet::new();
    for approval in approvals.iter() {
        if approval.content.appr_req_hash != event_proposal_hash {
            log::error!("Invalid Event Proposal Hash in Approval");
            continue;
        }
        match approval.verify() {
            Ok(_) => (),
            Err(_) => {
                log::error!("Invalid Signature Detected");
                continue;
            }
        }
        if !signers.contains(&approval.signature.signer) {
            log::error!("Signer {} not allowed", approval.signature.signer.to_str());
            continue;
        }
        if !actual_signers.insert(approval.signature.signer.clone()) {
            log::error!(
                "Signer {} in more than one validation signature",
                approval.signature.signer.to_str()
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
    validation_proof: &ValidationProof,
) -> Result<(), LedgerError> {
    let mut actual_signers = HashSet::new();
    for signature in signatures.iter() {
        let signer = signature.signer.clone();
        match signature.verify(validation_proof) {
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
        return Err(LedgerError::NotEnoughSignatures("buenas tardes".into()));
    }
    Ok(())
}
