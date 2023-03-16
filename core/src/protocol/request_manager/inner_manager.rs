use std::collections::{HashMap, HashSet};

use time::OffsetDateTime;
use crate::commons::{
    bd::TapleDB,
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    models::{
        approval_signature::{ApprovalResponse, ApprovalResponseContent},
        event_request::{EventRequest, EventRequestType, RequestData},
        notification::Notification, timestamp::TimeStamp,
    },
};
use crate::governance::GovernanceInterface;
use crate::message::{MessageConfig, MessageTaskCommand};

use super::super::{
    command_head_manager::{
        manager::CommandManagerInterface, self_signature_manager::SelfSignatureInterface,
    },
    errors::{RequestManagerError, ResponseError},
    protocol_message_manager::ProtocolManagerMessages,
};

use super::{Acceptance, ApprovalRequest, RequestManagerResponse, VotationType};

pub trait NotifierInterface {
    fn request_reached(&self, id: &str, subject_id: &str);
    fn negative_quorum_reached(&self, id: &str, subject_id: &str);
    fn quorum_reached(&self, id: &str, subject_id: &str);
}

pub struct RequestNotifier {
    sender: tokio::sync::broadcast::Sender<Notification>,
}

impl RequestNotifier {
    pub fn new(sender: tokio::sync::broadcast::Sender<Notification>) -> Self {
        Self { sender }
    }
}

impl NotifierInterface for RequestNotifier {
    fn request_reached(&self, id: &str, subject_id: &str) {
        let _ = self.sender.send(Notification::RequestReached {
            request_id: id.clone().to_owned(),
            subject_id: subject_id.clone().to_owned(),
            default_message: format!(
                "Se ha recibido la petición {} del sujeto {}",
                id, subject_id
            ),
        });
    }
    fn negative_quorum_reached(&self, id: &str, subject_id: &str) {
        let _ = self
            .sender
            .send(Notification::RequestNegativeQuroumReached {
                request_id: id.clone().to_owned(),
                subject_id: subject_id.clone().to_owned(),
                default_message: format!(
                    "Petición {} del sujeto {} ha sido rechazada por la red",
                    id, subject_id
                ),
            });
    }
    fn quorum_reached(&self, id: &str, subject_id: &str) {
        let _ = self.sender.send(Notification::RequestQuroumReached {
            request_id: id.clone().to_owned(),
            subject_id: subject_id.clone().to_owned(),
            default_message: format!(
                "Petición {} del sujeto {} ha sido aprovada por la red",
                id, subject_id
            ),
        });
    }
}

const ONE_MINUTE: u32 = 1000 * 60;

pub struct InnerManager<Database, N, C, G, S>
where
    Database: TapleDB,
    N: NotifierInterface,
    C: CommandManagerInterface,
    G: GovernanceInterface,
    S: SelfSignatureInterface,
{
    db: Database,
    notifier: N,
    command_api: C,
    governance_api: G,
    request_table: HashMap<DigestIdentifier, (DigestIdentifier, bool)>, // RequestID -> SubjectID + isAdmin
    to_approval_request: HashMap<DigestIdentifier, (EventRequest, u64)>, // SubjectID -> Data
    request_stack: HashMap<DigestIdentifier, (EventRequest, u64, HashSet<ApprovalResponse>)>, // SubjectID -> Queque
    signature_manager: S,
    pass_votation: VotationType,
}

impl<
        Database: TapleDB,
        N: NotifierInterface,
        C: CommandManagerInterface,
        G: GovernanceInterface,
        S: SelfSignatureInterface,
    > InnerManager<Database, N, C, G, S>
{
    pub fn new(
        db: Database,
        notifier: N,
        command_api: C,
        governance_api: G,
        signature_manager: S,
        pass_votation: VotationType,
    ) -> Self {
        Self {
            db,
            notifier,
            command_api,
            governance_api,
            request_table: HashMap::new(),
            request_stack: HashMap::new(),
            to_approval_request: HashMap::new(),
            signature_manager,
            pass_votation,
        }
    }

    pub async fn init(&mut self) -> Result<(), RequestManagerError> {
        // You must query the DB to check pending requests and forward them
        // Votes are not being saved until Quorum is reached so right now you should
        // the voting process should be redone right now
        // TODO: Store the votes together with the request
        let all_request = self.db.get_all_request();
        let mut tasks = Vec::new();
        for request in all_request {
            let request_id = request.signature.content.event_content_hash.clone();
            let subject_id = match &request.request {
                EventRequestType::Create(_) => {
                    return Err(RequestManagerError::DatabaseCorrupted(
                        "Create Request found".into(),
                    ));
                }
                EventRequestType::State(data) => {
                    let subject_id = data.subject_id.clone();
                    let invokation_permissions = self
                        .governance_api
                        .check_invokation_permission(
                            subject_id.clone(),
                            request.signature.content.signer.clone(),
                            None,
                            None,
                        )
                        .await
                        .map_err(|e| RequestManagerError::RequestError(e))?;
                    if !invokation_permissions.0 || !invokation_permissions.1 {
                        self.db.del_request(&subject_id, &request_id);
                    }
                    subject_id
                }
            };
            let Some(subject) = self.db.get_subject(&subject_id) else {
                return Err(RequestManagerError::DatabaseCorrupted(
                    "Subject of request stored not found".into(),
                ));
            };
            self.request_table
                .insert(request_id, (subject_id.clone(), true));
            let subject_data = subject.subject_data.unwrap();
            let expected_sn = subject_data.sn;
            self.request_stack.insert(
                subject_id.clone(),
                (request.clone(), expected_sn, HashSet::new()),
            );
            let mut targets = self
                .governance_api
                .get_approvers(request.clone())
                .await
                .map_err(|e| RequestManagerError::RequestError(e))?;
            if self.signature_manager.check_if_signature_present(&targets) {
                let controller_id = self.signature_manager.get_own_identifier();
                targets.remove(&controller_id);
                self.to_approval_request
                    .insert(subject_id, (request.clone(), expected_sn));
            }
            tasks.push(Self::send_approval_request(
                Vec::from_iter(targets),
                request,
                expected_sn,
                ONE_MINUTE,
            ));
        }
        Ok(())
    }

    pub fn get_pending_request(&self) -> RequestManagerResponse {
        RequestManagerResponse::GetPendingRequests(
            self.to_approval_request
                .iter()
                .map(|(_, (request, _))| request.clone())
                .collect(),
        )
    }

    pub fn get_single_request(&self, id: DigestIdentifier) -> RequestManagerResponse {
        let Some((subject_id, _)) = self.request_table.get(&id) else {
            return RequestManagerResponse::GetSingleRequest(Err(ResponseError::RequestNotFound));
        };
        let request = self.to_approval_request.get(&subject_id);
        match request {
            Some((request, _)) => RequestManagerResponse::GetSingleRequest(Ok(request.clone())),
            None => RequestManagerResponse::GetSingleRequest(Err(ResponseError::RequestNotFound)),
        }
    }

    pub async fn process_request(
        &mut self,
        request: EventRequest,
    ) -> Result<
        (
            RequestManagerResponse,
            Option<MessageTaskCommand<ProtocolManagerMessages>>,
        ),
        RequestManagerError,
    > {
        // It must be checked whether it is a subject or a governance.
        //   - If it is a subject, it is up to the commandManager to handle the request.
        //   - If it is a governance, it will only be treated if the request is a State type.
        // Check if the request already exists in the system. If it does, it is discarded.
        // Check whether a request is already being processed for the subject in question.
        //   - If yes, the operation is denied. With the inclusion of the queue this will change.
        //  The request must be cryptographically validated. That is, check that the signature is correct.
        // Store the request in DB as well as in memory. A list of the votes received must also be managed.
        // Voting is requested from the governance members.
        //   - It is sent to all network members with a high timeout (about 1 minute).
        //   - The vote should include the expected SN assumption. This will serve to help the other nodes
        //     to detect if they are out of sync.
        let check_signatures = match request.check_signatures() {
            Ok(_) => Ok(()),
            Err(error) => {
                log::error!("Error: {}", error);
                Err(error)
            }
        };
        check_signatures.map_err(|_| RequestManagerError::SignVerificationFailed)?;
        match &request.request {
            EventRequestType::State(data) => {
                let subject_id = data.subject_id.clone();
                let invokation_permissions = self
                    .governance_api
                    .check_invokation_permission(
                        subject_id.clone(),
                        request.signature.content.signer.clone(),
                        None,
                        None,
                    )
                    .await;
                if let Err(request_error) = invokation_permissions {
                    return Ok((
                        RequestManagerResponse::CreateRequest(Err(
                            ResponseError::GovernanceError {
                                source: request_error,
                            },
                        )),
                        None,
                    ));
                }
                let invokation_permissions = invokation_permissions.unwrap();
                if !invokation_permissions.0 {
                    // It is not a valid invokator
                    return Ok((
                        RequestManagerResponse::CreateRequest(Err(ResponseError::InvalidCaller)),
                        None,
                    ));
                }
                // Check if subject is present
                let Some(subject) = self.db.get_subject(&data.subject_id) else {
                    return Ok((
                        RequestManagerResponse::CreateRequest(Err(
                            ResponseError::SubjectNotFound,
                        )),
                        None,
                    ));
                };
                let Some(_) = &subject.keys else {
                    return Ok((
                        RequestManagerResponse::CreateRequest(Err(
                            ResponseError::NotOwnerOfSubject,
                        )),
                        None,
                    ));
                };
                let Some(subject_data) = subject.subject_data.clone() else {
                    return Ok((
                        RequestManagerResponse::CreateRequest(Err(
                            ResponseError::SubjectNotFound,
                        )),
                        None,
                    ));
                };
                // Check if the subject is being validating
                if subject.ledger_state.negociating_next {
                    return Ok((
                        RequestManagerResponse::CreateRequest(Err(
                            ResponseError::SubjectBeingValidated,
                        )),
                        None
                    ))
                }
                let schema_id = subject_data.schema_id.clone();
                let Ok(schema) = self
                    .governance_api
                    .get_schema(&subject_data.governance_id, &schema_id)
                    .await else {
                        return Ok((
                            RequestManagerResponse::CreateRequest(Err(
                                ResponseError::SchemaNotFound(schema_id),
                            )),
                            None
                        ))
                    };
                let Ok(_) = request.check_against_schema(&schema, &subject) else {
                    return Ok((
                        RequestManagerResponse::CreateRequest(Err(
                            ResponseError::EventRequestVerificationFailed,
                        )),
                        None
                    ))
                };
                if invokation_permissions.1 {
                    // Must be voted on
                    if self.request_stack.contains_key(&data.subject_id) {
                        return Ok((
                            RequestManagerResponse::CreateRequest(Err(
                                ResponseError::SubjectNotAvailable,
                            )),
                            None,
                        ));
                    }
                    // TODO: It should be managed in memory also the votes. Ask whether to implement it in this version
                    self.db.set_request(&data.subject_id, request.clone());
                    let (_, mut targets) = self
                        .governance_api
                        .check_quorum_request(request.clone(), HashSet::new())
                        .await
                        .unwrap();
                    let expected_sn = subject_data.sn
                        + if subject.ledger_state.negociating_next {
                            2
                        } else {
                            1
                        };
                    self.request_table.insert(
                        request.signature.content.event_content_hash.clone(),
                        (data.subject_id.clone(), true),
                    );
                    if self.signature_manager.check_if_signature_present(&targets) {
                        self.to_approval_request
                            .insert(data.subject_id.clone(), (request.clone(), expected_sn));
                        self.notifier.quorum_reached(
                            &request.signature.content.event_content_hash.to_str(),
                            &data.subject_id.to_str(),
                        );
                        let controller_id = self.signature_manager.get_own_identifier();
                        targets.remove(&controller_id);
                    }
                    let id = request.signature.content.event_content_hash.clone();
                    self.request_stack.insert(
                        data.subject_id.clone(),
                        (request.clone(), expected_sn, HashSet::new()),
                    );
                    match self.pass_votation {
                        VotationType::Normal => (),
                        VotationType::AlwaysAccept => {
                            self.process_approval_resolve(&id, Acceptance::Accept)
                                .await?;
                        }
                        VotationType::AlwaysReject => {
                            self.process_approval_resolve(&id, Acceptance::Reject)
                                .await?;
                        }
                    }
                    // TODO: Make TIEMOUT configurable
                    let subject_id = data.subject_id.clone();
                    Ok((
                        RequestManagerResponse::CreateRequest(Ok(RequestData {
                            request: request.request.clone(),
                            request_id: id.to_str(),
                            subject_id: Some(subject_id.to_str()),
                            sn: None,
                            timestamp: request.timestamp.clone(),
                        })),
                        Some(Self::send_approval_request(
                            Vec::from_iter(targets),
                            request,
                            subject_data.sn + 1,
                            ONE_MINUTE,
                        )),
                    ))
                } else {
                    // Does not have to wait for approval
                    let id = request.signature.content.event_content_hash.clone();
                    let timestamp = request.timestamp.clone();
                    let result = self.command_api.create_event(request.clone(), true).await;
                    if result.is_err() {
                        return Ok((
                            RequestManagerResponse::CreateRequest(Err(result.unwrap_err())),
                            None,
                        ));
                    }
                    let event = result.unwrap();
                    let sn = event.event_content.sn;
                    return Ok((
                        RequestManagerResponse::CreateRequest(Ok(RequestData {
                            request: request.request,
                            request_id: id.to_str(),
                            subject_id: Some(event.event_content.subject_id.to_str()),
                            sn: Some(sn),
                            timestamp,
                        })),
                        None,
                    ));
                }
            }
            EventRequestType::Create(_) => {
                // TODO: It is necessary to respond to the API with the messages it expects.
                // TODO: It may also be possible to modify the latter. Note that the following must be taken into account
                // CommandManager messages. It would be possible to relegate the responsibility to the commandManager,
                // but it would imply to stop using the API or to add a new method that takes the property of the response
                // oneshot. Pending, in turn, determine the performance of this channel at this level, since it is only
                // present in the Manager
                let id = request.signature.content.event_content_hash.clone();
                // TODO: For now, we always accept subject creation events
                let result = self.command_api.create_event(request.clone(), true).await;
                if result.is_err() {
                    return Ok((
                        RequestManagerResponse::CreateRequest(Err(result.unwrap_err())),
                        None,
                    ));
                }
                let event = result.unwrap();
                return Ok((
                    RequestManagerResponse::CreateRequest(Ok(RequestData {
                        request: request.request,
                        request_id: id.to_str(),
                        subject_id: Some(event.event_content.subject_id.to_str()),
                        sn: Some(0),
                        timestamp: request.timestamp,
                    })),
                    None,
                ));
            }
        }
    }

    pub async fn process_approval(
        &mut self,
        approval: ApprovalResponse,
    ) -> Result<
        (
            RequestManagerResponse,
            Option<MessageTaskCommand<ProtocolManagerMessages>>,
        ),
        RequestManagerError,
    > {
        // The cryptographic validity of the vote received is checked.
        // Quroum is stored and checked. This Quorum must be both positive and negative.
        // If a negative quorum is reached, the user is notified.
        //   - It would also be possible to generate the event and keep it in the chain to record it.
        //   - Delete the request.
        //   - Cancel task.
        // If a positive quorum is reached.
        //   - Send the request to CommandManager and wait for its response (ASK).
        //   - Once the response is received, the request is deleted from the DB and the user is notified.
        //   - Cancel task.
        // If no Quroum is reached
        //   - Update task, so that no more messages are sent to the node that has already cast its vote.
        // TODO: Error control
        let Some((subject_id, is_admin)) = self.request_table.get(&approval.content.event_request_hash).cloned() else {
            return Ok((RequestManagerResponse::Vote(Err(ResponseError::RequestNotFound)), None));
        };
        if !is_admin {
            return Ok((
                RequestManagerResponse::Vote(Err(ResponseError::NotOwnerOfSubject)),
                None,
            ));
        }
        let Some((mut request, expected_sn, mut approvals)) = self.request_stack.get_mut(&subject_id).cloned() else {
            return Ok((RequestManagerResponse::Vote(Err(ResponseError::RequestNotFound)), None));
        };
        approval
            .check_signatures()
            .map_err(|_| RequestManagerError::SignVerificationFailed)?;
        if approval.content.expected_sn != expected_sn {
            return Ok((
                RequestManagerResponse::Vote(Err(ResponseError::UnexpectedSN)),
                None,
            ));
        }
        if request.signature.content.event_content_hash != approval.content.event_request_hash {
            return Ok((
                RequestManagerResponse::Vote(Err(ResponseError::InvalidHash)),
                None,
            ));
        }
        approvals.insert(approval);
        let request_id = request.signature.content.event_content_hash.clone();
        let data = if let EventRequestType::State(data) = &request.request {
            data.clone()
        } else {
            unreachable!()
        };
        let (quorum_status, targets) = self
            .governance_api
            .check_quorum_request(request.clone(), approvals.clone())
            .await
            .unwrap();
        match quorum_status {
            crate::governance::RequestQuorum::Accepted => {
                self.request_stack.remove(&data.subject_id);
                self.request_table.remove(&request_id);
                request.approvals = approvals
                    .into_iter()
                    .filter(|v| {
                        if let Acceptance::Accept = v.content.approval_type {
                            true
                        } else {
                            false
                        }
                    })
                    .collect();
                self.notifier
                    .quorum_reached(&request_id.to_str(), &data.subject_id.to_str());
                // TODO: What to do if CommandManager fails
                let result = self
                    .command_api
                    .create_event(
                        request,
                        true,
                    )
                    .await;
                if result.is_ok() {
                    // It is now safe to delete the Request from the database, as the event has been created.
                    self.db.del_request(&subject_id, &request_id);
                }
                match result {
                    Err(ResponseError::ComunnicationClosed) => {
                        Err(RequestManagerError::ComunicationWithCommandManagerClosed)
                    }
                    _ => Ok((
                        RequestManagerResponse::Vote(Ok(())),
                        Some(Self::cancel_approval_request(&request_id)),
                    )),
                }
            }
            crate::governance::RequestQuorum::Rejected => {
                self.request_stack.remove(&data.subject_id);
                self.request_table.remove(&request_id);
                self.db.del_request(&subject_id, &request_id);
                self.notifier
                    .negative_quorum_reached(&request_id.to_str(), &data.subject_id.to_str());
                let result = self
                    .command_api
                    .create_event(
                        request,
                        false,
                    )
                    .await;
                if result.is_ok() {
                    // It is now safe to delete the Request from the database, as the event has been created.
                    self.db.del_request(&subject_id, &request_id);
                }
                match result {
                    Err(ResponseError::ComunnicationClosed) => {
                        Err(RequestManagerError::ComunicationWithCommandManagerClosed)
                    }
                    _ => Ok((
                        RequestManagerResponse::Vote(Ok(())),
                        Some(Self::cancel_approval_request(&request_id)),
                    )),
                }
            }
            crate::governance::RequestQuorum::Processing => {
                // TODO: Self-management of the signature
                self.request_stack.entry(data.subject_id).and_modify(|e| {
                    e.2 = approvals;
                });
                Ok((
                    RequestManagerResponse::Vote(Ok(())),
                    Some(Self::send_approval_request(
                        Vec::from_iter(targets),
                        request,
                        expected_sn,
                        ONE_MINUTE,
                    )),
                ))
            }
        }
    }

    fn cancel_approval_request(
        request_id: &DigestIdentifier,
    ) -> MessageTaskCommand<ProtocolManagerMessages> {
        MessageTaskCommand::<ProtocolManagerMessages>::Cancel(request_id.to_str())
    }

    fn send_approval_request(
        targets: Vec<KeyIdentifier>,
        request: EventRequest,
        sn: u64,
        timeout: u32,
    ) -> MessageTaskCommand<ProtocolManagerMessages> {
        let request_id = request.signature.content.event_content_hash.clone();
        let config = MessageConfig {
            timeout: timeout,
            replication_factor: 1f64,
        };
        // TODO: Study the ID to be assigned. Indefinite or finite task.
        let msg = ProtocolManagerMessages::ApprovalRequest(ApprovalRequest {
            request: request,
            expected_sn: sn,
        });
        MessageTaskCommand::<ProtocolManagerMessages>::Request(
            Some(request_id.to_str()),
            msg,
            targets,
            config,
        )
    }

    pub async fn process_approval_request(
        &mut self,
        approval_request: ApprovalRequest,
    ) -> Result<
        (
            RequestManagerResponse,
            Option<MessageTaskCommand<ProtocolManagerMessages>>,
        ),
        RequestManagerError,
    > {
        // It is checked if the subject is possessed.
        // The cryptographic validity of the request is checked.
        // We check if we are synchronized.
        //   - It involves consulting the subject.
        //   - We will be able to process the request as long as the SN is equal to or less than the current one.
        // The request is not signed on the fly, but stored in memory. It does not need to be stored in DB.
        // A notification is sent to the user.

        // TODO: Attempt to ascertain the identity of the sender. One could sign with the subject to ensure that the message
        // comes only from the controller. This would allow us to keep a single vote in the DB. It could also be possible
        // with timestamp.
        let id = approval_request
            .request
            .signature
            .content
            .event_content_hash
            .clone();
        approval_request
            .request
            .check_signatures()
            .map_err(|_| RequestManagerError::SignVerificationFailed)?;
        let EventRequestType::State(data) = approval_request.request.request.clone() else {
            return Ok((RequestManagerResponse::ApprovalRequest(Err(ResponseError::RequestTypeError)), None));
        };
        let None = self.request_stack.get(&data.subject_id) else {
            // TODO: It is possible that the vote has already been generated and we can respond.
            return Ok((RequestManagerResponse::ApprovalRequest(Err(ResponseError::RequestAlreadyKnown)), None));
        };
        let Some(subject) = self.db.get_subject(&data.subject_id) else {
            return Ok((RequestManagerResponse::ApprovalRequest(Err(ResponseError::SubjectNotFound)), None));
        };
        let Some(subject_data) = subject.subject_data else {
            return Ok((RequestManagerResponse::ApprovalRequest(Err(ResponseError::SubjectNotFound)), None));
        };
        let invokation_permissions = self
            .governance_api
            .check_invokation_permission(
                data.subject_id.clone(),
                approval_request.request.signature.content.signer.clone(),
                None,
                None,
            )
            .await
            .map_err(|e| RequestManagerError::RequestError(e))?;
        if !invokation_permissions.0 {
            return Ok((
                RequestManagerResponse::ApprovalRequest(Err(ResponseError::InvalidCaller)),
                None,
            ));
        }
        if invokation_permissions.1 {
            if approval_request.expected_sn == subject_data.sn + 1 {
                // TODO: Revise according to phase 2
                self.request_table
                    .insert(id.clone(), (subject_data.subject_id.clone(), false));
                self.notifier
                    .request_reached(&id.to_str(), &subject_data.subject_id.to_str());
                self.to_approval_request.insert(
                    subject_data.subject_id,
                    (approval_request.request, approval_request.expected_sn),
                );
            } else if approval_request.expected_sn > subject_data.sn + 1 {
                return Ok((
                    RequestManagerResponse::ApprovalRequest(Err(
                        ResponseError::NoSynchronizedSubject,
                    )),
                    None,
                ));
            } else {
                return Ok((
                    RequestManagerResponse::ApprovalRequest(Err(
                        ResponseError::EventAlreadyOnChain,
                    )),
                    None,
                ));
            }
        } else {
            return Ok((
                RequestManagerResponse::ApprovalRequest(Err(ResponseError::ApprovalNotNeeded)),
                None,
            ));
        }
        match self.pass_votation {
            VotationType::Normal => {
                // TODO: Pending the management of the data structure.
                Ok((RequestManagerResponse::ApprovalRequest(Ok(())), None))
            }
            VotationType::AlwaysAccept => {
                self.process_approval_resolve(&id, Acceptance::Accept).await
            }
            VotationType::AlwaysReject => {
                self.process_approval_resolve(&id, Acceptance::Reject).await
            }
        }
    }

    pub async fn process_approval_resolve(
        &mut self,
        id: &DigestIdentifier,
        acceptance: Acceptance,
    ) -> Result<
        (
            RequestManagerResponse,
            Option<MessageTaskCommand<ProtocolManagerMessages>>,
        ),
        RequestManagerError,
    > {
        // It is checked if there is a request with the indicated ID.
        // The vote is cast to the sender node.
        // Is the request deleted? -> It could arrive again and should not be processed.
        let msg = {
            let Some((subject_id, is_admin)) = self.request_table.get(id) else {
                return Ok((RequestManagerResponse::VoteResolve(Err(ResponseError::RequestNotFound)), None));
            };
            let Some((request, expected_sn)) = self.to_approval_request.get(&subject_id) else {
                return Ok((RequestManagerResponse::VoteResolve(Err(ResponseError::VoteNotNeeded)), None));
            };
            let None = self.db.get_event(subject_id, *expected_sn) else {
                self.to_approval_request.remove(&subject_id);
                self.request_table.remove(id);
                return Ok((RequestManagerResponse::VoteResolve(Err(ResponseError::EventAlreadyOnChain)), None));
            };
            let signature = self
                .signature_manager
                .sign(&(id.clone(), acceptance.clone(), expected_sn))
                .map_err(|_| RequestManagerError::SignError)?;
            let approval_signature = ApprovalResponseContent {
                signer: self.signature_manager.get_own_identifier(),
                event_request_hash: request.signature.content.event_content_hash.clone(),
                approval_type: acceptance,
                expected_sn: *expected_sn,
                timestamp: TimeStamp::now(),
            };
            let target = self
                .db
                .get_subject(&subject_id)
                .expect("Subject is there")
                .subject_data
                .expect("Hay Subject Data")
                .owner;
            // let target = request.signature.content.signer.clone();
            self.to_approval_request.remove(&subject_id);
            if !is_admin {
                self.request_table.remove(id);
                let config = MessageConfig {
                    timeout: 0,
                    replication_factor: 1f64,
                };
                let msg = ProtocolManagerMessages::Vote(ApprovalResponse {
                    content: approval_signature,
                    signature: signature.signature,
                });
                Some(MessageTaskCommand::<ProtocolManagerMessages>::Request(
                    None,
                    msg,
                    vec![target],
                    config,
                ))
            } else {
                // If you are an administrator, you do not send a message to yourself.
                let approval = ApprovalResponse {
                    content: approval_signature,
                    signature: signature.signature,
                };
                self.process_approval(approval).await?.1
            }
        };
        Ok((RequestManagerResponse::VoteResolve(Ok(())), msg))
    }
}
