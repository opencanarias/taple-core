use std::collections::{HashMap, HashSet};

use crate::commons::{
    bd::{db::DB, TapleDB},
    errors::SubjectError,
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{
        event::Event,
        event_content::EventContent,
        event_request::{EventRequest, EventRequestType},
        signature::Signature,
        state::{LedgerState, Subject, SubjectData},
    },
};
use crate::governance::{GovernanceAPI, GovernanceInterface};
use serde_json::Value;

use super::super::errors::{CryptoError, LedgerManagerError};

use super::{CommandManagerResponse, EventSN};

pub struct Ledger {
    ledger_state: HashMap<DigestIdentifier, LedgerState>,
    candidate_cache: HashMap<DigestIdentifier, HashMap<u64, Event>>,
    repo_access: DB,
    id: KeyIdentifier,
    governance_api: GovernanceAPI,
}

impl Ledger {
    pub fn new(repo_access: DB, id: KeyIdentifier, governance_api: GovernanceAPI) -> Self {
        Self {
            ledger_state: HashMap::new(),
            candidate_cache: HashMap::new(),
            repo_access,
            id,
            governance_api,
        }
    }

    pub fn get_subject(&self, subject_id: &DigestIdentifier) -> Option<Subject> {
        self.repo_access.get_subject(subject_id)
    }

    pub fn get_all_subjects(&self) -> Vec<Subject> {
        self.repo_access.get_all_subjects()
    }

    pub fn get_event_from_candidate_cache(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Option<&Event> {
        match self.candidate_cache.get(subject_id) {
            None => None,
            Some(candidate_list) => candidate_list.get(&sn),
        }
    }

    pub fn set_negociating_true(
        &mut self,
        subject_id: &DigestIdentifier,
    ) -> Result<(), SubjectError> {
        match self.ledger_state.get_mut(subject_id) {
            Some(ledger_state) => {
                ledger_state.negociating_next = true;
                Ok(())
            }
            None => Err(SubjectError::SubjectNotFound),
        }
    }

    pub fn genesis_event(
        &mut self,
        event_request: EventRequest,
        governance_version: u64,
        subject_schema: &Value,
        approved: bool,
    ) -> Result<CommandManagerResponse, SubjectError> {
        if self.id != event_request.signature.content.signer {
            return Err(SubjectError::NotOwnerOfSubject);
        }
        // TODO: Here I always accept genesis events
        let res =
            event_request.create_subject_from_request(governance_version, subject_schema, approved);
        if res.is_err() {
            log::error!("ERROR: {:?}", res);
            return Err(res.unwrap_err());
        }
        let (subject, event) = res.unwrap();
        let subject_id = event.event_content.subject_id.clone();
        self.repo_access.set_event(&subject_id, event.clone());
        let ledger_state = subject.ledger_state.clone();
        self.repo_access.set_subject(&subject_id, subject);
        self.ledger_state.insert(subject_id, ledger_state.clone());
        Ok(CommandManagerResponse::CreateEventResponse(
            event,
            ledger_state,
        ))
    }

    pub fn state_event(
        &mut self,
        event_request: EventRequest,
        governance_version: u64,
        mut subject: Subject,
        subject_schema: &Value,
        approved: bool,
    ) -> Result<CommandManagerResponse, SubjectError> {
        let subject_id = if let EventRequestType::State(state_req) = event_request.request.clone() {
            state_req.subject_id
        } else {
            return Err(SubjectError::NotStateEvent);
        };
        if subject.subject_data.is_none() {
            return Err(SubjectError::SubjectHasNoData);
        } else if subject.ledger_state.negociating_next {
            return Err(SubjectError::EventAlreadyProcessing);
        }
        let prev_event_hash = match self
            .repo_access
            .get_event(&subject_id, subject.subject_data.as_ref().unwrap().sn)
        {
            Some(event) => event.signature.content.event_content_hash,
            None => return Err(SubjectError::EventAlreadyAppliedNotFound),
        };
        let event = event_request.get_event_from_state_request(
            &subject,
            prev_event_hash,
            governance_version,
            subject_schema,
            approved,
        )?;
        self.repo_access.set_event(&subject_id, event.clone());
        self.repo_access.set_negociating_true(&subject_id)?;
        subject.ledger_state.negociating_next = true;
        self.set_negociating_true(&subject_id)?;
        Ok(CommandManagerResponse::CreateEventResponse(
            event,
            subject.ledger_state,
        ))
    }

    pub fn apply_event_sourcing(
        &mut self,
        event: Event,
        subject_schema: &Value,
    ) -> Result<LedgerState, LedgerManagerError> {
        // Apply Event Sourcing and update LedgerState
        let ev_sn = event.event_content.sn;
        let subject_id = event.event_content.subject_id.clone();
        let mut ledger_state = self
            .ledger_state
            .get(&subject_id)
            .expect("Ya comprobamos antes que existía")
            .to_owned();
        ledger_state.negociating_next = false;
        ledger_state.head_sn = Some(ev_sn);
        let mut prev_hash = event.signature.content.event_content_hash.clone();
        let event_sourcing = self.repo_access.apply_event_sourcing(event.event_content);
        if let Err(e) = event_sourcing {
            return Err(LedgerManagerError::SubjectError(e));
        }
        if ledger_state.head_candidate_sn.is_some() {
            for sn in (ev_sn + 1)..=(ledger_state.head_candidate_sn.unwrap()) {
                // Check for next event (if there is no more we stop and modify head)
                if let Some(ev) = self.repo_access.get_event(&subject_id, sn) {
                    // Check that it engages with the preloader
                    if prev_hash != ev.event_content.previous_hash {
                        return Err(LedgerManagerError::CryptoError(CryptoError::Conflict));
                    }
                    // Check subject state and schema
                    match self.check_future_state_hash(ev.event_content.clone(), subject_schema) {
                        Ok(_) => (),
                        Err(e) => return Err(e),
                    }
                    // Event Sourcing and updating head and prev_hash
                    ledger_state.head_sn = Some(sn);
                    prev_hash = ev.signature.content.event_content_hash.clone();
                    let event_sourcing = self.repo_access.apply_event_sourcing(ev.event_content);
                    if let Err(e) = event_sourcing {
                        return Err(LedgerManagerError::SubjectError(e));
                    }
                } else {
                    // There are no more events and the end is not reached
                    return Ok(ledger_state.to_owned());
                }
            }
            // If the end is reached, the head_candidate is removed and put as head (because they are all there)
            ledger_state.head_candidate_sn = None;
        }
        self.ledger_state.insert(subject_id, ledger_state.clone());
        Ok(ledger_state.to_owned())
    }

    pub fn put_signatures(
        &mut self,
        signatures: HashSet<Signature>,
        sn: u64,
        subject_id: DigestIdentifier,
        quorum: bool,
        subject_schema: &Value,
    ) -> Result<LedgerState, LedgerManagerError> {
        // Check whether signatures are required (Head, Head+1 or Candidate)
        let ledger_state = self.get_ledger_state(&subject_id);
        if ledger_state.is_none()
            || (ledger_state.is_some()
                && ledger_state.unwrap().head_candidate_sn.is_some()
                && ledger_state.unwrap().head_candidate_sn.unwrap() < sn)
            || (ledger_state.is_some()
                && ledger_state.unwrap().head_candidate_sn.is_none()
                && ledger_state.unwrap().head_sn.unwrap() + 1 < sn)
        {
            // Case: Possible candidate
            self.put_signatures_posible_candidate(signatures, sn, subject_id, quorum)
        } else if (ledger_state.is_some()
            && ledger_state.unwrap().head_candidate_sn.is_some()
            && ledger_state.unwrap().head_candidate_sn.unwrap() == sn)
            || (ledger_state.is_some()
                && ledger_state.unwrap().head_sn.is_some()
                && ledger_state.unwrap().head_sn.unwrap() == sn)
        {
            // Case: signatures for candidate or for head (same protocol)
            self.repo_access.set_signatures(&subject_id, sn, signatures);
            Ok(self.get_ledger_state(&subject_id).unwrap().to_owned())
        } else if ledger_state.is_some()
            && ledger_state.unwrap().head_sn.is_some()
            && ledger_state.unwrap().head_sn.unwrap() + 1 == sn
            && ledger_state.unwrap().negociating_next
        {
            // Case: signatures for Head + 1
            self.put_signatures_negociating_event(
                signatures,
                sn,
                subject_id,
                quorum,
                subject_schema,
            )
        } else {
            // They are not necessary
            Err(LedgerManagerError::SignaturesNotNeeded)
        }
        // Add and check if it reaches quorum (outside)
        // Update status if quorum is reached (in other inner function)
    }

    pub fn put_signatures_posible_candidate(
        &mut self,
        signatures: HashSet<Signature>,
        sn: u64,
        subject_id: DigestIdentifier,
        quorum: bool,
    ) -> Result<LedgerState, LedgerManagerError> {
        match self.candidate_cache.get(&subject_id) {
            None => return Err(LedgerManagerError::SubjectNotFound),
            Some(candidate_list) => {
                if let Some(ev) = candidate_list.get(&sn) {
                    self.repo_access.set_signatures(&subject_id, sn, signatures);
                    if quorum {
                        match self.get_ledger_state(&subject_id) {
                            None => {
                                // If there is no previous subject
                                let new_ledger_state = LedgerState {
                                    head_sn: None,
                                    head_candidate_sn: Some(sn),
                                    negociating_next: false,
                                };
                                // Update ledger state, add event and add subject in database
                                let subject = Subject::new_empty(new_ledger_state.clone());
                                self.repo_access.set_subject(&subject_id, subject);
                                self.repo_access.set_event(&subject_id, ev.clone());
                                self.ledger_state
                                    .insert(subject_id, new_ledger_state.clone());
                                Ok(new_ledger_state)
                            }
                            Some(ledger_state) => {
                                // If there is a previous subject (it is an older candidate or a new one)
                                let mut ledger_state = ledger_state.to_owned();
                                ledger_state.head_candidate_sn = Some(sn);
                                self.repo_access.set_event(&subject_id, ev.clone());
                                self.ledger_state.insert(subject_id, ledger_state.clone());
                                Ok(ledger_state)
                            }
                        }
                    } else {
                        Ok(match self.get_ledger_state(&subject_id) {
                            Some(ledger_state) => ledger_state.clone(),
                            None => LedgerState::default(),
                        })
                    }
                } else {
                    Err(LedgerManagerError::EventNotFound(LedgerState::default()))
                }
            }
        }
    }

    pub fn put_signatures_negociating_event(
        &mut self,
        signatures: HashSet<Signature>,
        sn: u64,
        subject_id: DigestIdentifier,
        quorum: bool,
        subject_schema: &Value,
    ) -> Result<LedgerState, LedgerManagerError> {
        let ledger_state = self.get_ledger_state(&subject_id).unwrap().to_owned();
        self.repo_access
            .set_signatures(&subject_id, sn, signatures.clone());
        if quorum {
            // Change ledger_state and event sourcing
            let event = self
                .get_event_from_db(&subject_id, sn)
                .expect("Tiene que haber evento");
            match self.apply_event_sourcing(event, subject_schema) {
                Ok(ledger_state) => {
                    Ok(ledger_state)
                },
                Err(e) => {
                    Err(e)
                },
            }
        } else {
            Ok(ledger_state)
        }
    }

    pub fn set_posible_candidate(&mut self, event: Event) -> bool {
        match self
            .candidate_cache
            .get_mut(&event.event_content.subject_id)
        {
            Some(hs) => hs.insert(event.event_content.sn, event).is_none(),
            None => {
                let mut hs = HashMap::new();
                hs.insert(event.event_content.sn, event.clone());
                self.candidate_cache
                    .insert(event.event_content.subject_id, hs);
                true
            }
        }
    }

    /*pub fn check_validator(&self, signers: HashSet<KeyIdentifier>) -> bool {
        signers.contains(&self.id)
    }*/

    pub fn get_ledger_state(&self, subject_id: &DigestIdentifier) -> Option<&LedgerState> {
        self.ledger_state.get(subject_id)
    }

    pub fn init(&mut self) -> Result<CommandManagerResponse, LedgerManagerError> {
        let head_and_candidates = self.repo_access.get_all_heads();
        let mut cache: HashMap<DigestIdentifier, LedgerState> = HashMap::new();
        for (subject_id, ledger_state) in head_and_candidates.into_iter() {
            cache.insert(subject_id, ledger_state);
        }
        self.ledger_state = cache;
        Ok(CommandManagerResponse::InitResponse(
            self.ledger_state.clone(),
        ))
    }

    pub fn get_head_sn(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<Option<u64>, LedgerManagerError> {
        match self.get_ledger_state(subject_id) {
            Some(ledger_state) => Ok(ledger_state.head_sn),
            None => Err(LedgerManagerError::SubjectNotFound),
        }
    }

    pub fn get_event_from_db(&self, subject_id: &DigestIdentifier, sn: u64) -> Option<Event> {
        self.repo_access.get_event(subject_id, sn)
    }

    pub fn get_signatures_from_db(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Option<HashSet<Signature>> {
        self.repo_access.get_signatures(subject_id, sn)
    }

    pub fn get_event(
        &self,
        subject_id: &DigestIdentifier,
        sn: EventSN,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        match self.get_head_sn(subject_id) {
            Ok(head_sn_opt) => match head_sn_opt {
                Some(head_sn) => match sn {
                    EventSN::SN(num) => {
                        if num <= head_sn {
                            Ok(CommandManagerResponse::GetEventResponse {
                                event: self.repo_access.get_event(subject_id, num).unwrap(),
                                ledger_state: self.get_ledger_state(subject_id).unwrap().to_owned(),
                            })
                        } else if self.get_ledger_state(subject_id).unwrap().negociating_next
                            && num == head_sn + 1
                        {
                            // Case head + 1
                            Ok(CommandManagerResponse::GetEventResponse {
                                event: self.repo_access.get_event(subject_id, num).unwrap(),
                                ledger_state: self.get_ledger_state(subject_id).unwrap().to_owned(),
                            })
                        } else if self
                            .get_ledger_state(subject_id)
                            .unwrap()
                            .head_candidate_sn
                            .is_some()
                            && self
                                .get_ledger_state(subject_id)
                                .unwrap()
                                .head_candidate_sn
                                .unwrap()
                                == num
                        {
                            Ok(CommandManagerResponse::GetEventResponse {
                                event: self.repo_access.get_event(subject_id, num).unwrap(),
                                ledger_state: self.get_ledger_state(subject_id).unwrap().to_owned(),
                            })
                        } else {
                            Err(LedgerManagerError::EventNotFound(
                                self.get_ledger_state(subject_id).unwrap().to_owned(),
                            ))
                        }
                    }
                    EventSN::HEAD => Ok(CommandManagerResponse::GetEventResponse {
                        event: self.repo_access.get_event(subject_id, head_sn).unwrap(),
                        ledger_state: self.get_ledger_state(subject_id).unwrap().to_owned(),
                    }),
                },
                None => Err(LedgerManagerError::EventNotFound(
                    self.get_ledger_state(subject_id).unwrap().to_owned(),
                )),
            },
            Err(e) => Err(e),
        }
    }

    pub fn get_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: EventSN,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        match self.get_head_sn(subject_id) {
            Ok(head_sn_opt) => match head_sn_opt {
                Some(head_sn) => match sn {
                    EventSN::SN(num) => {
                        if num <= head_sn {
                            Ok(CommandManagerResponse::GetSignaturesResponse {
                                signatures: match self.repo_access.get_signatures(subject_id, num) {
                                    Some(signatures) => signatures,
                                    None => HashSet::new(),
                                },
                                ledger_state: self.get_ledger_state(subject_id).unwrap().to_owned(),
                            })
                        } else if self.get_ledger_state(subject_id).unwrap().negociating_next
                            && num == head_sn + 1
                        {
                            // Case head + 1
                            Ok(CommandManagerResponse::GetSignaturesResponse {
                                signatures: match self.repo_access.get_signatures(subject_id, num) {
                                    Some(signatures) => signatures,
                                    None => HashSet::new(),
                                },
                                ledger_state: self.get_ledger_state(subject_id).unwrap().to_owned(),
                            })
                        } else if self
                            .get_ledger_state(subject_id)
                            .unwrap()
                            .head_candidate_sn
                            .is_some()
                            && self
                                .get_ledger_state(subject_id)
                                .unwrap()
                                .head_candidate_sn
                                .unwrap()
                                == num
                        {
                            Ok(CommandManagerResponse::GetSignaturesResponse {
                                signatures: match self.repo_access.get_signatures(subject_id, num) {
                                    Some(signatures) => signatures,
                                    None => HashSet::new(),
                                },
                                ledger_state: self.get_ledger_state(subject_id).unwrap().to_owned(),
                            })
                        } else {
                            Err(LedgerManagerError::EventNotFound(
                                self.get_ledger_state(subject_id).unwrap().to_owned(),
                            ))
                        }
                    }
                    EventSN::HEAD => Ok(CommandManagerResponse::GetSignaturesResponse {
                        signatures: match self.repo_access.get_signatures(subject_id, head_sn) {
                            Some(signatures) => signatures,
                            None => HashSet::new(),
                        },
                        ledger_state: self.get_ledger_state(subject_id).unwrap().to_owned(),
                    }),
                },
                None => Err(LedgerManagerError::EventNotFound(
                    self.get_ledger_state(subject_id).unwrap().to_owned(),
                )),
            },
            Err(e) => Err(e),
        }
    }

    pub fn get_signers(
        &self,
        subject_id: &DigestIdentifier,
        sn: EventSN,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        match self.get_ledger_state(subject_id) {
            None => Err(LedgerManagerError::SubjectNotFound),
            Some(ledger_state) => match sn {
                EventSN::SN(num) => {
                    let signers: HashSet<KeyIdentifier> =
                        match self.repo_access.get_signatures(subject_id, num) {
                            Some(signatures) => signatures
                                .iter()
                                .map(|signature| signature.content.signer.clone())
                                .collect(),
                            None => HashSet::new(),
                        };
                    Ok(CommandManagerResponse::GetSignersResponse {
                        signers,
                        ledger_state: ledger_state.to_owned(),
                    })
                }
                EventSN::HEAD => match ledger_state.head_sn {
                    None => Err(LedgerManagerError::EventNotFound(LedgerState::default())),
                    Some(head_sn) => {
                        let signers: HashSet<KeyIdentifier> =
                            match self.repo_access.get_signatures(subject_id, head_sn) {
                                Some(signatures) => signatures
                                    .iter()
                                    .map(|signature| signature.content.signer.clone())
                                    .collect(),
                                None => HashSet::new(),
                            };
                        Ok(CommandManagerResponse::GetSignersResponse {
                            signers,
                            ledger_state: ledger_state.to_owned(),
                        })
                    }
                },
            },
        }
    }

    pub fn set_new_subj_ev0(
        &mut self,
        event: Event,
        subject_schema: &Value,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        let subject = match Subject::new(
            &event.event_content,
            event.signature.content.signer.clone(),
            None,
            subject_schema,
        ) {
            Ok(subj) => subj,
            Err(e) => return Err(LedgerManagerError::SubjectError(e)),
        };
        self.repo_access
            .set_event(&event.event_content.subject_id, event.clone());
        self.repo_access
            .set_subject(&event.event_content.subject_id, subject);
        let mut ledger_state = LedgerState {
            head_sn: Some(0),
            head_candidate_sn: if let Some(ledger_s) =
                self.get_ledger_state(&event.event_content.subject_id)
            {
                ledger_s.head_candidate_sn
            } else {
                None
            },
            negociating_next: false,
        };
        let subject_id = event.event_content.subject_id.clone();
        let mut prev_hash = event.signature.content.event_content_hash.clone();
        if ledger_state.head_candidate_sn.is_some() {
            for sn in 1..=(ledger_state.head_candidate_sn.unwrap()) {
                // Check for next event (if there is no more we stop and modify head)
                if let Some(ev) = self.repo_access.get_event(&subject_id, sn) {
                    // Check that it engages with the prev
                    if prev_hash != ev.event_content.previous_hash {
                        return Err(LedgerManagerError::CryptoError(CryptoError::Conflict));
                    }
                    // Check subject state and schema
                    match self.check_future_state_hash(ev.event_content.clone(), subject_schema) {
                        Ok(_) => (),
                        Err(e) => return Err(e),
                    }
                    // Event sourcing and updating head and prev_hash
                    ledger_state.head_sn = Some(sn);
                    prev_hash = ev.signature.content.event_content_hash.clone();
                    let event_sourcing = self.repo_access.apply_event_sourcing(ev.event_content);
                    if let Err(e) = event_sourcing {
                        return Err(LedgerManagerError::SubjectError(e));
                    }
                } else {
                    // No more events and no end is reached
                    self.ledger_state
                        .insert(event.event_content.subject_id, ledger_state.clone());
                    return Ok(CommandManagerResponse::PutEventResponse { ledger_state });
                }
            }
            // If the end is reached, the head_candidate is removed and put as head (because they are all there)
            ledger_state.head_candidate_sn = None;
        }
        self.ledger_state
            .insert(event.event_content.subject_id, ledger_state.clone());
        Ok(CommandManagerResponse::PutEventResponse { ledger_state })
    }

    fn check_future_state_hash(
        &self,
        event_content: EventContent,
        subject_schema: &Value,
    ) -> Result<(), LedgerManagerError> {
        // Check if the future state of the subject matches
        let subject_id = event_content.subject_id.clone();
        let subject = self
            .repo_access
            .get_subject(&subject_id)
            .expect("Tiene que haber sujeto aqui porque hay ledger state");
        match subject.get_future_subject_content_hash(event_content.clone(), subject_schema) {
            Ok(future_subject_state_hash) => {
                if future_subject_state_hash != event_content.state_hash {
                    return Err(LedgerManagerError::SubjectError(
                        SubjectError::EventSourcingHashNotEqual,
                    ));
                } else {
                    Ok(())
                }
            }
            Err(e) => return Err(LedgerManagerError::SubjectError(e)),
        }
    }

    pub fn insert_head_plus_one(
        &mut self,
        event: Event,
        subject_schema: &Value,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        // Check for a cryptographic match with the previous one
        let subject_id = event.event_content.subject_id.clone();
        let prev_event = self
            .repo_access
            .get_event(&event.event_content.subject_id, event.event_content.sn - 1)
            .expect("Tiene que haber evento anterior");
        if prev_event.signature.content.event_content_hash != event.event_content.previous_hash {
            return Err(LedgerManagerError::CryptoError(CryptoError::Conflict));
        }
        // Check if the future state of the subject matches
        match self.check_future_state_hash(event.event_content.clone(), subject_schema) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }
        // Add event
        self.repo_access
            .set_event(&event.event_content.subject_id, event.clone());
        // Check if there is a candidate and if it is reached
        let mut ledger_state = self
            .ledger_state
            .get_mut(&subject_id)
            .expect("Ya comprobamos antes que existía");
        if ledger_state.head_candidate_sn.is_some() {
            // Apply for new event
            match self.apply_event_sourcing(event, subject_schema) {
                Ok(ledger_state) => Ok(CommandManagerResponse::PutEventResponse {
                    ledger_state: ledger_state.to_owned(),
                }),
                Err(e) => Err(e),
            }
        } else {
            match self.repo_access.set_negociating_true(&subject_id) {
                Ok(_) => (),
                Err(e) => return Err(LedgerManagerError::SubjectError(e)),
            };
            ledger_state.negociating_next = true;
            Ok(CommandManagerResponse::PutEventResponse {
                ledger_state: ledger_state.to_owned(),
            })
        }
    }

    pub async fn put_event(
        &mut self,
        event: Event,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        match self
            .governance_api
            .get_schema(
                &event.event_content.metadata.governance_id,
                &event.event_content.metadata.schema_id,
            )
            .await
        {
            Ok(subject_schema) => {
                let ledger_state = self.get_ledger_state(&event.event_content.subject_id);
                if ledger_state.is_some() {
                    let ledger_state = ledger_state.unwrap().clone();
                    // Head+1, Posible candidate, event 0 of Candidate
                    if event.event_content.sn == 0 && ledger_state.head_sn.is_none() {
                        // Add new event 0 of new Subject
                        self.set_new_subj_ev0(event, &subject_schema)
                    } else if ledger_state.head_sn.is_some()
                        && ledger_state.head_sn.unwrap() + 1 == event.event_content.sn
                        && !ledger_state.negociating_next
                    {
                        // Case Head + 1 (negotiating or add directly if there is a candidate)
                        self.insert_head_plus_one(event, &subject_schema)
                    } else if (ledger_state.head_candidate_sn.is_none()
                        && ledger_state.head_sn.is_some()
                        && ledger_state.head_sn.unwrap() + 1 < event.event_content.sn)
                        || (ledger_state.head_candidate_sn.is_some()
                            && ledger_state.head_candidate_sn.unwrap() < event.event_content.sn)
                    {
                        // Case posible candidate
                        if self.set_posible_candidate(event) {
                            Ok(CommandManagerResponse::PutEventResponse {
                                ledger_state: ledger_state,
                            })
                        } else {
                            Err(LedgerManagerError::EventAlreadyExists)
                        }
                    } else {
                        Err(LedgerManagerError::EventNotNeeded(ledger_state))
                    }
                } else {
                    // Posible candidate for new Subject or event 0
                    // Check if i am validator
                    if event.event_content.sn == 0 {
                        // Add new event 0 of new Subject
                        self.set_new_subj_ev0(event, &subject_schema)
                    } else {
                        // Add new posible candidate
                        if self.set_posible_candidate(event) {
                            Ok(CommandManagerResponse::PutEventResponse {
                                ledger_state: LedgerState {
                                    head_sn: None,
                                    head_candidate_sn: None,
                                    negociating_next: false,
                                },
                            })
                        } else {
                            Err(LedgerManagerError::EventAlreadyExists)
                        }
                    }
                }
            }
            Err(e) => Err(LedgerManagerError::GovernanceError(e)),
        }
    }

    pub async fn put_signatures_top(
        &mut self,
        signatures: HashSet<Signature>,
        sn: u64,
        subject_id: DigestIdentifier,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        // Check ledger state
        let ledger_state = self.get_ledger_state(&subject_id);
        let mut event = self.get_event_from_db(&subject_id, sn);
        if event.is_none() {
            match self.get_event_from_candidate_cache(&subject_id, sn) {
                Some(event_cach) => event = Some(event_cach.to_owned()),
                None => {
                    return Err(LedgerManagerError::EventNotFound(
                        if ledger_state.is_none() {
                            LedgerState {
                                head_sn: None,
                                head_candidate_sn: None,
                                negociating_next: false,
                            }
                        } else {
                            ledger_state.unwrap().to_owned()
                        },
                    ))
                }
            }
        }
        let event = event.unwrap();
        // Obtain a list of validators to see if signatures are required.
        let validators_list = match self.governance_api.get_validators(event.clone()).await {
            Ok(vl) => vl,
            Err(e) => {
                return Err(LedgerManagerError::GovernanceError(e));
            }
        };
        // Check if the validators of the signatures are matched
        for signature in signatures.iter() {
            if !validators_list.contains(&signature.content.signer) {
                return Err(LedgerManagerError::InvalidValidator);
            }
            // Check cryptographic validity of signatures
            if signature.content.event_content_hash != event.signature.content.event_content_hash {
                return Err(LedgerManagerError::CryptoError(CryptoError::InvalidHash));
            }
        }
        // Obtain old signatures, join them with the new ones and check if a quorum is reached.
        let total_signers: HashSet<KeyIdentifier> = {
            match self
                .get_signatures_from_db(&event.event_content.subject_id, event.event_content.sn)
            {
                Some(prev_signatures) => signatures
                    .union(&prev_signatures)
                    .map(|x| x.content.signer.clone())
                    .collect(),
                None => signatures
                    .iter()
                    .map(|x| x.content.signer.clone())
                    .collect(),
            }
        };
        // Check Quorum
        let (quorum, signers_left) = match self
            .governance_api
            .check_quorum(event.clone(), &total_signers)
            .await
        {
            Ok(result) => result,
            Err(e) => return Err(LedgerManagerError::GovernanceError(e)),
        };
        let metadata = event.event_content.metadata;
        let subject_schema = match self
            .governance_api
            .get_schema(&metadata.governance_id, &metadata.schema_id)
            .await
        {
            Ok(result) => result,
            Err(e) => return Err(LedgerManagerError::GovernanceError(e)),
        };
        match self.put_signatures(signatures, sn, subject_id, quorum, &subject_schema) {
            Ok(ledger_state) => Ok(CommandManagerResponse::PutSignaturesResponse {
                sn,
                signers: total_signers,
                signers_left,
                ledger_state,
            }),
            Err(e) => Err(e),
        }
    }

    pub async fn create_event(
        &mut self,
        event_request: EventRequest,
        approved: bool,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        if let Err(e) = event_request.check_signatures() {
            return Err(LedgerManagerError::CryptoError(CryptoError::Event(e)));
        }
        //TODO: Check that the invoker has permission to launch request/create subject (in my case)
        match event_request.request.clone() {
            EventRequestType::Create(create_request) => {
                let governance_version = match self
                    .governance_api
                    .get_governance_version(&create_request.governance_id)
                    .await
                {
                    Ok(result) => result,
                    Err(e) => return Err(LedgerManagerError::GovernanceError(e)),
                };
                let subject_schema = match self
                    .governance_api
                    .get_schema(&create_request.governance_id, &create_request.schema_id)
                    .await
                {
                    Ok(result) => result,
                    Err(e) => return Err(LedgerManagerError::GovernanceError(e)),
                };
                match self.genesis_event(
                    event_request,
                    governance_version,
                    &subject_schema,
                    approved,
                ) {
                    Ok(create_event_response) => Ok(create_event_response),
                    Err(e) => Err(LedgerManagerError::SubjectError(e)),
                }
            }
            EventRequestType::State(state_request) => {
                let subject = self.get_subject(&state_request.subject_id);
                if subject.is_some() && subject.clone().unwrap().subject_data.is_some() {
                    let subject = subject.unwrap();
                    let subject_data = subject.subject_data.clone().unwrap();
                    let governance_version = match self
                        .governance_api
                        .get_governance_version(&subject_data.governance_id)
                        .await
                    {
                        Ok(result) => result,
                        Err(e) => return Err(LedgerManagerError::GovernanceError(e)),
                    };
                    let subject_schema = match self
                        .governance_api
                        .get_schema(&subject_data.governance_id, &subject_data.schema_id)
                        .await
                    {
                        Ok(result) => result,
                        Err(e) => return Err(LedgerManagerError::GovernanceError(e)),
                    };
                    match self.state_event(
                        event_request,
                        governance_version,
                        subject,
                        &subject_schema,
                        approved,
                    ) {
                        Ok(create_event_response) => Ok(create_event_response),
                        Err(e) => Err(LedgerManagerError::SubjectError(e)),
                    }
                } else {
                    Err(LedgerManagerError::SubjectError(
                        SubjectError::SubjectNotFound,
                    ))
                }
            }
        }
    }

    pub fn get_subject_top(
        &self,
        subject_id: DigestIdentifier,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        match self.get_subject(&subject_id) {
            Some(subject) => match subject.subject_data {
                Some(sd) => Ok(CommandManagerResponse::GetSubjectResponse { subject: sd }),
                None => Err(LedgerManagerError::SubjectNotFound),
            },
            None => Err(LedgerManagerError::SubjectNotFound),
        }
    }

    pub fn get_subjects(
        &self,
        namespace: &str,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        let subjects = self.get_all_subjects();
        let subjects = subjects
            .into_iter()
            .filter(|subject| {
                subject.subject_data.is_some()
                    && subject.subject_data.as_ref().unwrap().namespace == namespace
            })
            .map(|subject| subject.subject_data.unwrap())
            .collect::<Vec<SubjectData>>();
        Ok(CommandManagerResponse::GetSubjectsResponse { subjects })
    }

    pub fn get_subjects_raw(
        &self,
        _namespace: &str,
    ) -> Result<CommandManagerResponse, LedgerManagerError> {
        let subjects = self.get_all_subjects();
        Ok(CommandManagerResponse::GetSubjectsRawResponse { subjects })
    }
}
