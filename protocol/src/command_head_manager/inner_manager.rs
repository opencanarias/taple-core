use std::collections::HashSet;

use commons::{
    config::TapleSettings,
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    models::{
        event::Event,
        event_request::{EventRequest, EventRequestType, RequestPayload},
        signature::Signature,
        state::LedgerState,
    },
};
use governance::{error::RequestError, GovernanceInterface};
use ledger::{
    errors::LedgerManagerError,
    ledger_manager::{EventSN, LedgerInterface},
};
use log::info;
use message::MessageTaskCommand;

use crate::errors::{EventCreationError, ResponseError};
use crate::{errors::ProtocolErrors, protocol_message_manager::ProtocolManagerMessages};

use super::{
    self_signature_manager::SelfSignatureInterface, utils, CommandGetEventResponse,
    CommandGetSignaturesResponse, CommandManagerResponses, Conflict, EventId, SendResponse,
};

#[derive(PartialEq, Debug)]
pub(crate) enum EventTypes {
    Head,
    Candidate,
    NoConsolidated,
    Tail,
    Processing,
    Unchainned,
    NewSubject,
}

pub struct InnerManager<L, G, S, N>
where
    L: LedgerInterface,
    G: GovernanceInterface,
    S: SelfSignatureInterface,
    N: NotifierInterface,
{
    ledger: L,
    governance: G,
    signature_manager: S,
    notifier: N,
    // Task Settings
    replication_factor: f64,
    timeout: u32,
    // Other
    own_request: Vec<(HashSet<Signature>, DigestIdentifier, u64)>,
}

pub trait NotifierInterface {
    fn subject_created(&self, id: &str);
    fn event_created(&self, id: &str, sn: u64);
    fn quorum_reached(&self, id: &str, sn: u64);
    fn event_signed(&self, id: &str, sn: u64);
    fn subject_synchronized(&self, id: &str, sn: u64);
}

impl<
        L: LedgerInterface,
        G: GovernanceInterface,
        S: SelfSignatureInterface,
        N: NotifierInterface,
    > InnerManager<L, G, S, N>
{
    pub fn new(
        ledger: L,
        governance: G,
        signature_manager: S,
        notifier: N,
        replication_factor: f64,
        timeout: u32,
    ) -> Self {
        Self {
            ledger,
            governance,
            signature_manager,
            notifier,
            replication_factor,
            timeout,
            own_request: Vec::new(),
        }
    }

    pub fn change_settings(&mut self, settings: &TapleSettings) {
        self.replication_factor = settings.node.replication_factor;
        self.timeout = settings.node.timeout;
        self.signature_manager.change_settings(settings);
    }

    pub fn get_pending_request(&mut self) -> Option<(HashSet<Signature>, DigestIdentifier, u64)> {
        self.own_request.pop()
    }

    pub async fn init(&mut self) -> Result<(), ProtocolErrors> {
        let ledger_response = self.ledger.init().await;
        let mut result = Vec::new();
        match ledger_response {
            Ok(list) => {
                for (subject_id, state) in list {
                    if state.head_sn.is_some() {
                        let head_sn = state.head_sn.unwrap();
                        if state.head_candidate_sn.is_some() {
                            // Synchronization pending
                            let candidate = state.head_candidate_sn.unwrap();
                            let signers = self.get_signers(candidate, subject_id.clone()).await?;
                            let tasks = self.process_candidate(&state, &subject_id, signers)?;
                            result.extend(tasks);
                        } else {
                            // Obtaining validators
                            let targets = {
                                let (event, _) =
                                    self.ledger.get_event(&subject_id, EventSN::HEAD).await?;
                                self.governance.get_validators(event).await?
                            };
                            // Check if HEAD is complete
                            let signers = self.get_signers(head_sn, subject_id.clone()).await?;
                            let tasks = self
                                .process_head(
                                    targets.clone(),
                                    signers,
                                    &state,
                                    subject_id.clone(),
                                    head_sn,
                                )
                                .await?;
                            result.extend(tasks);
                            if state.negociating_next {
                                // Negotiation pending
                                let signers =
                                    self.get_signers(head_sn + 1, subject_id.clone()).await?;
                                let tasks = self
                                    .process_processing(targets, signers, subject_id, head_sn + 1)
                                    .await?;
                                result.extend(tasks);
                            }
                        }
                    } else if state.head_candidate_sn.is_some() {
                        // New subject
                        let candidate = state.head_candidate_sn.unwrap();
                        let signers: HashSet<KeyIdentifier> =
                            self.get_signers(candidate, subject_id.clone()).await?;
                        let tasks = self.proccess_new_subject(Vec::from_iter(signers), subject_id);
                        result.extend(tasks);
                    }
                }
            }
            Err(error) => {
                return Err(ProtocolErrors::LedgerError {
                    source: error.into(),
                });
            }
        }
        Ok(())
    }

    pub async fn get_event(
        &self,
        data: &EventId,
        subject_id: &DigestIdentifier,
    ) -> Result<(CommandGetEventResponse, Option<u64>), ProtocolErrors> {
        let sn = match data {
            EventId::SN { sn } => EventSN::SN(*sn),
            EventId::HEAD => EventSN::HEAD,
        };
        let event_response = self.ledger.get_event(subject_id, sn).await;
        match event_response {
            Ok((event, ledger_state)) => {
                // If we have the event we return it
                Ok((CommandGetEventResponse::Data(event), ledger_state.head_sn))
            }
            Err(LedgerManagerError::SubjectNotFound) => Ok((
                CommandGetEventResponse::Conflict(Conflict::SubjectNotFound),
                None,
            )),
            Err(LedgerManagerError::EventNotFound(_)) => Ok((
                CommandGetEventResponse::Conflict(Conflict::EventNotFound),
                None,
            )),
            _ => {
                unreachable!()
            }
        }
    }

    pub async fn get_subjects(
        &self,
        namespace: String,
    ) -> Result<CommandManagerResponses, ProtocolErrors> {
        let subject_response = self.ledger.get_subjects(namespace).await;
        match subject_response {
            Ok(subjects) => Ok(CommandManagerResponses::GetSubjectsResponse(Ok(subjects))),
            Err(LedgerManagerError::ChannelClosed) => {
                Ok(CommandManagerResponses::GetSubjectsResponse(Err(
                    ResponseError::LedgerChannelClosed,
                )))
            }
            Err(error) => Err(ProtocolErrors::LedgerError {
                source: error.into(),
            }),
        }
    }

    pub async fn get_subject(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<CommandManagerResponses, ProtocolErrors> {
        let subject_response = self.ledger.get_subject(subject_id).await;
        match subject_response {
            Ok(subject) => Ok(CommandManagerResponses::GetSingleSubjectResponse(Ok(
                subject,
            ))),
            Err(LedgerManagerError::ChannelClosed) => {
                Ok(CommandManagerResponses::GetSingleSubjectResponse(Err(
                    ResponseError::LedgerChannelClosed,
                )))
            }
            Err(LedgerManagerError::SubjectNotFound) => {
                Ok(CommandManagerResponses::GetSingleSubjectResponse(Err(
                    ResponseError::SubjectNotFound,
                )))
            }
            Err(error) => Err(ProtocolErrors::LedgerError {
                source: error.into(),
            }),
        }
    }

    fn get_event_type(sn: u64, state: &LedgerState) -> EventTypes {
        if state.head_sn.is_some() {
            let head_sn = state.head_sn.unwrap();
            if sn == head_sn {
                if state.head_candidate_sn.is_some() {
                    return EventTypes::NoConsolidated;
                } else {
                    return EventTypes::Head;
                }
            }
            if sn < head_sn {
                return EventTypes::Tail;
            }
            if state.head_candidate_sn.is_some() {
                let candidate = state.head_candidate_sn.unwrap();
                if sn == candidate {
                    return EventTypes::Candidate;
                }
            } else {
                if sn == head_sn + 1 && state.negociating_next {
                    return EventTypes::Processing;
                }
            }
            return EventTypes::Unchainned;
        } else {
            if state.head_candidate_sn.is_some() {
                return EventTypes::NewSubject;
            } else {
                return EventTypes::Unchainned;
            }
        }
    }

    async fn get_signers(
        &self,
        sn: u64,
        subject_id: DigestIdentifier,
    ) -> Result<HashSet<KeyIdentifier>, ProtocolErrors> {
        let signers_response = self.ledger.get_signers(subject_id, EventSN::SN(sn)).await;
        match signers_response {
            Ok((signers, ..)) => Ok(signers),
            Err(error) => Err(ProtocolErrors::LedgerError {
                source: error.into(),
            }),
        }
    }

    async fn sign_event_if_needed(
        &mut self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signers: HashSet<KeyIdentifier>,
    ) -> Result<(), ProtocolErrors> {
        if !self.signature_manager.check_if_signature_present(&signers) {
            // Get ledger event
            if let CommandGetEventResponse::Data(event) =
                self.get_event(&EventId::SN { sn }, &subject_id).await?.0
            {
                self.notifier.event_signed(&subject_id.to_str(), sn);
                info!("Subject {} event {} signed", subject_id.to_str(), sn);
                let signatures = HashSet::<Signature>::from_iter(vec![self
                    .signature_manager
                    .sign(&event.event_content)?]);
                self.own_request
                    .insert(0, (signatures, subject_id.clone(), sn));
            }
        }
        Ok(())
    }

    async fn process_head(
        &mut self,
        mut targets: HashSet<KeyIdentifier>,
        signers: HashSet<KeyIdentifier>,
        ledger_state: &LedgerState,
        subject_id: DigestIdentifier,
        sn: u64,
    ) -> Result<Vec<MessageTaskCommand<ProtocolManagerMessages>>, ProtocolErrors> {
        targets.remove(&self.signature_manager.get_own_identifier());
        let signers_left: HashSet<KeyIdentifier> = targets.difference(&signers).cloned().collect();
        let mut result = Vec::<MessageTaskCommand<ProtocolManagerMessages>>::new();
        if !ledger_state.negociating_next {
            // Nothing is being negotiated
            self.notifier.quorum_reached(&subject_id.to_str(), sn);
            let id = format!("S{}PROCESSING", subject_id.to_str());
            result.push(utils::build_cancel_request(id));
        }
        if signers_left.len() == 0 {
            // HEAD COMPLETE
            // Cancel signature petition for SN
            let id = ["S".to_owned(), subject_id.to_str(), "HEAD".to_owned()].join("");
            result.push(utils::build_cancel_request(id));
        } else {
            // HEAD NOT COMPLETE
            // Update signature petition for SN
            let id = format!("S{}HEAD", subject_id.to_str());
            result.push(utils::build_request_signature(
                signers.clone(),
                Vec::from_iter(targets),
                subject_id.clone(),
                sn,
                Some(id),
                self.replication_factor,
                self.timeout,
            ));
        }
        self.sign_event_if_needed(&subject_id, sn, signers).await?;
        Ok(result)
    }

    async fn process_processing(
        &mut self,
        mut targets: HashSet<KeyIdentifier>,
        signers: HashSet<KeyIdentifier>,
        subject_id: DigestIdentifier,
        sn: u64,
    ) -> Result<Vec<MessageTaskCommand<ProtocolManagerMessages>>, ProtocolErrors> {
        targets.remove(&self.signature_manager.get_own_identifier());
        let id = format!("S{}PROCESSING", subject_id.to_str());
        self.sign_event_if_needed(&subject_id, sn, signers.clone())
            .await?;
        Ok(vec![utils::build_request_signature(
            signers,
            targets.into_iter().collect(),
            subject_id.clone(),
            sn,
            Some(id),
            self.replication_factor,
            self.timeout,
        )])
    }

    fn process_candidate(
        &self,
        ledger_state: &LedgerState,
        subject_id: &DigestIdentifier,
        mut signers: HashSet<KeyIdentifier>,
    ) -> Result<Vec<MessageTaskCommand<ProtocolManagerMessages>>, ProtocolErrors> {
        // Issue message to obtain no_consolidated + 1
        // Since we do not consider malicious agents, we can assume that the next block to be requested is HEAD + 1
        // If HEAD does not exist, we ask for 0
        signers.remove(&self.signature_manager.get_own_identifier());
        let sn_no_consolidated = if ledger_state.head_sn.is_some() {
            ledger_state.head_sn.unwrap() + 1
        } else {
            0
        };
        Ok(vec![utils::build_request_event_msg(
            Vec::from_iter(signers.into_iter()),
            subject_id.clone(),
            sn_no_consolidated,
            self.replication_factor,
        )])
    }

    fn proccess_new_subject(
        &self,
        signers: Vec<KeyIdentifier>,
        subject_id: DigestIdentifier,
    ) -> Vec<MessageTaskCommand<ProtocolManagerMessages>> {
        vec![utils::build_request_event_msg(signers, subject_id, 0, 1.0)]
    }

    async fn check_if_validator(
        &self,
        event: &Event,
        pk: &KeyIdentifier,
    ) -> Result<HashSet<KeyIdentifier>, ProtocolErrors> {
        // The event is checked to see if it is the genesis of a governance
        let validators = self.governance.get_validators(event.clone()).await?;
        if validators.contains(&pk) {
            Ok(validators)
        } else {
            Err(ProtocolErrors::NotValidator)
        }
    }

    pub async fn set_event(
        &mut self,
        data: Event,
    ) -> Result<
        (
            SendResponse,
            Vec<MessageTaskCommand<ProtocolManagerMessages>>,
        ),
        ProtocolErrors,
    > {
        // We check if we are validators of the event.
        let event_content = data.event_content.clone();

        let targets = {
            if event_content.metadata.governance_id.digest.is_empty() {
                None
            } else {
                let tmp = self
                    .check_if_validator(&data, &self.signature_manager.get_own_identifier())
                    .await;
                if tmp.is_err() {
                    log::debug!("Fallo al obtener validadores del evento");
                    return Ok((SendResponse::Invalid, vec![]));
                } else {
                    Some(tmp.unwrap())
                }
            }
        };
        // Check if event has the needed amount of approval to reach quorum
        // The valididy of each approval is check during the event creation
        if let EventRequestType::State(payload) = &event_content.event_request.request {
            // The LCE of a new governance needs special treatment
            let additional_payload = if event_content.metadata.schema_id == "governance" {
                // Governances can only have a JSON payload
                let RequestPayload::Json(payload) = &payload.payload else {
                    return Ok((SendResponse::Invalid, vec![]));
                };
                Some(payload.clone())
            } else {
                None
            };
            // TODO: Create a governance manager method specialized in testing the validity of phase 1 in phase 2
            // The current solution should be considered more of a stopgap
            let (is_valid_invokator, _approval_needed) = match self
                .governance
                .check_invokation_permission(
                    event_content.subject_id.clone(),
                    event_content.event_request.signature.content.signer.clone(),
                    additional_payload,
                    Some(event_content.metadata.clone()),
                )
                .await
            {
                Ok((is_valid_invokator, _approval_needed)) => {
                    (is_valid_invokator, _approval_needed)
                }
                Err(RequestError::ChannelClosed) => {
                    return Err(RequestError::ChannelClosed.into());
                }
                _ => {
                    return Ok((SendResponse::Invalid, vec![]));
                }
            };
            if !is_valid_invokator {
                // Invalid invokator. Can't accept the event
                return Ok((SendResponse::Invalid, vec![]));
            }
            // We should check the quorum status by ourselves but it needs the management of the governance version
        }
        let sn = data.event_content.sn;
        let ledger_response = self.ledger.put_event(data.clone()).await;
        match ledger_response {
            // TODO: Shuffle whether to do get_signers and get_targets in each branch to avoid some possible double calls
            Ok(ledger_state) => {
                let lo_del_match = Self::get_event_type(sn, &ledger_state);
                if EventTypes::Unchainned == lo_del_match {
                    return Ok((SendResponse::Valid, vec![]));
                }
                let targets = if targets.is_some() {
                    targets.unwrap()
                } else {
                    self.governance.get_validators(data).await?
                };
                let signers = self
                    .get_signers(sn, event_content.subject_id.clone())
                    .await?;
                let tasks = match Self::get_event_type(sn, &ledger_state) {
                    EventTypes::Head => {
                        self.notifier
                            .subject_created(&event_content.subject_id.to_str());
                        self.process_head(
                            // TODO: Check if it is likely to occur
                            targets,
                            signers,
                            &ledger_state,
                            event_content.subject_id,
                            sn,
                        )
                        .await?
                    }
                    EventTypes::NoConsolidated => {
                        let sn_next = ledger_state.head_sn.unwrap() + 1;
                        // The event is requested only to the signatories of the candidate
                        let signers = self
                            .get_signers(
                                ledger_state.head_candidate_sn.unwrap(),
                                event_content.subject_id.clone(),
                            )
                            .await?;
                        vec![utils::build_request_event_msg(
                            Vec::from_iter(signers),
                            event_content.subject_id,
                            sn_next,
                            self.replication_factor,
                        )]
                    }
                    EventTypes::Tail => {
                        // A new HEAD has been generated
                        // This is the case when a negotiand goes HEAD
                        self.notifier
                            .subject_synchronized(&event_content.subject_id.to_str(), sn);
                        let new_head_sn = ledger_state.head_sn.unwrap();
                        self.process_head(
                            targets,
                            signers,
                            &ledger_state,
                            event_content.subject_id,
                            new_head_sn,
                        )
                        .await?
                    }
                    EventTypes::Processing => {
                        self.notifier
                            .event_created(&event_content.subject_id.to_str(), sn);
                        self.process_processing(targets, signers, event_content.subject_id, sn)
                            .await?
                    }
                    _ => {
                        vec![]
                    }
                };
                Ok((SendResponse::Valid, tasks))
            }
            Err(LedgerManagerError::ChannelClosed) => Err(ProtocolErrors::UnexpectedLedgerResponse),
            Err(LedgerManagerError::CryptoError(_)) => Ok((SendResponse::Invalid, vec![])),
            _ => Ok((SendResponse::Invalid, vec![])),
        }
    }

    pub async fn set_signatures(
        &mut self,
        data: HashSet<Signature>,
        sn: u64,
        subject_id: DigestIdentifier,
    ) -> Result<
        (
            SendResponse,
            Vec<MessageTaskCommand<ProtocolManagerMessages>>,
        ),
        ProtocolErrors,
    > {
        if data.len() == 0 {
            return Ok((SendResponse::Valid, vec![]));
        }
        let ledger_response = self.ledger.put_signatures(&data, sn, &subject_id).await;
        match ledger_response {
            Ok((sn, signers, signers_left, ledger_state)) => {
                let tasks = match Self::get_event_type(sn, &ledger_state) {
                    EventTypes::Head => {
                        let mut result: Vec<MessageTaskCommand<ProtocolManagerMessages>> =
                            Vec::new();
                        // Check if governance has reached quorum and send notification signatures
                        let event = self
                            .ledger
                            .get_event(&subject_id, EventSN::SN(sn))
                            .await
                            .expect("Tiene que haber evento, si no habría fallado antes")
                            .0;
                        let all_validators = self.governance.get_validators(event.clone()).await?;
                        // Case in which it is a governance
                        if event.event_content.metadata.governance_id.digest.is_empty() {
                            let mut result1 = self
                                .process_governance_update(
                                    all_validators.clone(),
                                    signers.clone(),
                                    subject_id.clone(),
                                    sn,
                                )
                                .await?;
                            result.append(&mut result1);
                        }
                        let mut result2 = self
                            .process_head(
                                all_validators.clone(),
                                signers,
                                &ledger_state,
                                subject_id.clone(),
                                sn,
                            )
                            .await?;
                        result.append(&mut result2);
                        result
                    }
                    EventTypes::Processing => {
                        let targets = signers.union(&signers_left).cloned().collect();
                        self.process_processing(targets, signers, subject_id, sn)
                            .await?
                    }
                    EventTypes::Candidate => {
                        self.process_candidate(&ledger_state, &subject_id, signers)?
                    }
                    EventTypes::NewSubject => {
                        self.proccess_new_subject(Vec::from_iter(signers), subject_id)
                    }
                    _ => vec![],
                };
                Ok((SendResponse::Valid, tasks))
            }
            Err(LedgerManagerError::ChannelClosed) => Err(ProtocolErrors::UnexpectedLedgerResponse),
            Err(LedgerManagerError::CryptoError(_)) => Ok((SendResponse::Invalid, vec![])),
            _ => Ok((SendResponse::Invalid, vec![])),
        }
    }

    pub async fn get_signatures(
        &self,
        sn: EventId,
        requested_signatures: HashSet<KeyIdentifier>,
        subject_id: DigestIdentifier,
        sender_id: Option<KeyIdentifier>,
    ) -> Result<
        (
            CommandGetSignaturesResponse,
            Option<u64>,
            Option<MessageTaskCommand<ProtocolManagerMessages>>,
        ),
        ProtocolErrors,
    > {
        let signatures_response = self
            .ledger
            .get_signatues(
                subject_id.clone(),
                match sn {
                    EventId::HEAD => EventSN::HEAD,
                    EventId::SN { sn } => EventSN::SN(sn),
                },
            )
            .await;
        match signatures_response {
            Ok((signatures, ledger_state)) => {
                let (sn, signatures) = {
                    if let EventId::SN { sn } = sn {
                        (
                            Some(sn),
                            signatures
                                .into_iter()
                                .filter(|s| !requested_signatures.contains(&s.content.signer))
                                .collect(),
                        )
                    } else {
                        (ledger_state.head_sn, signatures)
                    }
                };
                Ok((CommandGetSignaturesResponse::Data(signatures), sn, None))
            }
            Err(LedgerManagerError::SubjectNotFound) => Ok((
                CommandGetSignaturesResponse::Conflict(Conflict::SubjectNotFound),
                None,
                if sender_id.is_some() {
                    let sender_id = sender_id.unwrap();
                    Some(utils::build_request_head(vec![sender_id], subject_id))
                } else {
                    None
                },
            )),
            Err(LedgerManagerError::EventNotFound(state)) => {
                let sn = if let EventId::SN { sn } = sn {
                    sn
                } else {
                    unreachable!()
                };
                let mut msg = None;
                if state.head_sn.is_some() && sender_id.is_some() {
                    let head_sn = state.head_sn.unwrap();
                    if sn == head_sn + 1 {
                        // Order HEAD + 1
                        msg = Some(utils::build_request_event_msg(
                            vec![sender_id.unwrap()],
                            subject_id.clone(),
                            sn,
                            1.0,
                        ));
                    } else if sn > head_sn + 1 {
                        // Order LCE
                        msg = Some(utils::build_request_head(
                            vec![sender_id.unwrap()],
                            subject_id,
                        ));
                    }
                }
                Ok((
                    CommandGetSignaturesResponse::Conflict(Conflict::EventNotFound),
                    None,
                    msg,
                ))
            }
            Err(error) => Err(ProtocolErrors::LedgerError {
                source: error.into(),
            }),
        }
    }

    async fn process_create_event(
        &mut self,
        event: &Event,
    ) -> Result<Vec<MessageTaskCommand<ProtocolManagerMessages>>, ProtocolErrors> {
        let mut validators = self.governance.get_validators(event.clone()).await?;
        if validators.contains(&self.signature_manager.get_own_identifier()) {
            self.sign_event_if_needed(
                &event.event_content.subject_id,
                event.event_content.sn,
                HashSet::new(),
            )
            .await?;
        }
        validators.remove(&self.signature_manager.get_own_identifier());
        Ok(vec![utils::build_request_signature(
            HashSet::new(),
            Vec::from_iter(validators),
            event.event_content.subject_id.clone(),
            event.event_content.sn,
            Some(
                [
                    "S".to_owned(),
                    event.event_content.subject_id.to_str(),
                    "PROCESSING".to_owned(),
                ]
                .join(""),
            ),
            self.replication_factor,
            self.timeout,
        )])
    }

    pub async fn create_event(
        &mut self,
        request: EventRequest,
        approved: bool,
    ) -> Result<
        (
            super::CreateEventResponse,
            Vec<MessageTaskCommand<ProtocolManagerMessages>>,
        ),
        ProtocolErrors,
    > {
        let (tasks, result) = match &request.request {
            EventRequestType::Create(_data) => {
                let ledger_response = self.ledger.create_event(request, approved).await;
                match ledger_response {
                    Ok((event, _)) => {
                        let tasks = self.process_create_event(&event).await?;
                        self.notifier
                            .subject_created(&event.event_content.subject_id.to_str());
                        info!(
                            "Subject {} created",
                            event.event_content.subject_id.to_str()
                        );
                        (tasks, super::CreateEventResponse::Event(event))
                    }
                    Err(_error) => (
                        vec![],
                        super::CreateEventResponse::Error(
                            EventCreationError::EventCreationFailed.into(),
                        ),
                    ),
                }
            }
            EventRequestType::State(data) => {
                let ledger_response = self.ledger.get_event(&data.subject_id, EventSN::HEAD).await;
                let state = match ledger_response {
                    Ok((ledger_state, ..)) => Ok(ledger_state),
                    Err(LedgerManagerError::SubjectNotFound) => Err(
                        super::CreateEventResponse::Error(ResponseError::SubjectNotFound),
                    ),
                    Err(error) => Err(super::CreateEventResponse::Error(error.into())),
                };
                if state.is_err() {
                    (vec![], state.unwrap_err())
                } else {
                    let ledger_response = self.ledger.create_event(request, approved).await;
                    match ledger_response {
                        Ok((event, _)) => {
                            let tasks = self.process_create_event(&event).await.unwrap();
                            self.notifier.event_created(
                                &event.event_content.subject_id.to_str(),
                                event.event_content.sn,
                            );
                            info!(
                                "Subject {} event {} created",
                                event.event_content.subject_id.to_str(),
                                event.event_content.sn
                            );
                            (tasks, super::CreateEventResponse::Event(event))
                        }
                        Err(_error) => (
                            vec![],
                            super::CreateEventResponse::Error(
                                EventCreationError::EventCreationFailed.into(),
                            ),
                        ),
                    }
                }
            }
        };
        Ok((result, tasks))
    }

    async fn process_governance_update(
        &mut self,
        mut targets: HashSet<KeyIdentifier>,
        _signers: HashSet<KeyIdentifier>,
        governance_id: DigestIdentifier,
        _sn: u64,
    ) -> Result<Vec<MessageTaskCommand<ProtocolManagerMessages>>, ProtocolErrors> {
        let mut result = Vec::<MessageTaskCommand<ProtocolManagerMessages>>::new();
        // Request list of Subjects controlled by us
        let subjects_data_vec = self
            .ledger
            .get_subjects_raw("namespace1".into())
            .await
            .expect("Actualmente esto no puede fallar");
        // Our cryptographic material for not asking anything of ourselves
        let own_mc = self.signature_manager.get_own_identifier();
        targets.remove(&own_mc);
        // We notify only those who have not signed the current governance, who we assume are already well
        let new_targets: Vec<KeyIdentifier> = targets.into_iter().collect();
        for data in subjects_data_vec.into_iter() {
            // Check if I have the subject
            let (ledger_state, Some(subject_data), Some(_)) = (data.ledger_state, data.subject_data, data.keys) else {
                    continue
                };
            if subject_data.owner != own_mc {
                continue;
            }
            if subject_data.governance_id != governance_id
                || subject_data.governance_id.digest.is_empty()
            {
                continue;
            }
            let (sn, head_or_processing) = if ledger_state.negociating_next {
                (ledger_state.head_sn.unwrap() + 1, "PROCESSING")
            } else {
                (ledger_state.head_sn.unwrap(), "HEAD")
            };
            // If I own the subject we ask for signatures for the last event.
            result.push(utils::build_request_signature(
                HashSet::new(),
                new_targets.clone(),
                subject_data.subject_id.clone(),
                sn,
                Some(format!(
                    "S{}{}",
                    subject_data.subject_id.to_str(),
                    head_or_processing
                )),
                self.replication_factor,
                self.timeout,
            ));
        }
        Ok(result)
    }

    pub async fn get_schema(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
    ) -> Result<CommandManagerResponses, ProtocolErrors> {
        match self.governance.get_schema(&governance_id, &schema_id).await {
            Ok(schema) => Ok(CommandManagerResponses::GetSchema(Ok(schema))),
            Err(error) => Err(ProtocolErrors::GovernanceError { source: error }),
        }
    }
}
