use std::collections::HashSet;

use super::{
    error::{APIInternalError, ApiError},
    inner_api::InnerApi,
    APICommands, ApiResponses, GetAllowedSubjects,
};
use super::{GetEvents, GetGovernanceSubjects};
#[cfg(feature = "approval")]
use crate::approval::manager::ApprovalAPI;
use crate::commons::models::request::TapleRequest;
use crate::commons::models::state::SubjectData;
use crate::event::manager::EventAPI;
use crate::ledger::manager::EventManagerAPI;
use crate::signature::Signature;
#[cfg(feature = "approval")]
use crate::ApprovalEntity;
use crate::ValidationProof;
use crate::{
    authorized_subjecs::manager::AuthorizedSubjectsAPI, signature::Signed, Event, EventRequest,
};
use crate::{
    commons::channel::{ChannelData, MpscChannel, SenderEnd},
    Notification,
};
use crate::{identifier::DigestIdentifier, DatabaseCollection, DB};
use crate::{KeyDerivator, KeyIdentifier};
use libp2p::PeerId;
use log::{error, info};
use tokio_util::sync::CancellationToken;

/// Object that allows interaction with a TAPLE node.
///
/// It has methods to perform all available read and write operations.
#[derive(Clone, Debug)]
pub struct Api {
    peer_id: PeerId,
    controller_id: String,
    public_key: Vec<u8>,
    sender: SenderEnd<APICommands, ApiResponses>,
}

impl Api {
    pub fn new(
        peer_id: PeerId,
        controller_id: String,
        public_key: Vec<u8>,
        sender: SenderEnd<APICommands, ApiResponses>,
    ) -> Self {
        Self {
            peer_id,
            controller_id,
            public_key,
            sender,
        }
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn controller_id(&self) -> &String {
        &self.controller_id
    }

    pub fn public_key(&self) -> &Vec<u8> {
        &self.public_key
    }

    pub async fn get_request(
        &self,
        request_id: DigestIdentifier,
    ) -> Result<TapleRequest, ApiError> {
        let response = self.sender.ask(APICommands::GetRequest(request_id)).await;
        if response.is_err() {
            log::debug!(
                "EN EL MODULE INTERFACE ES ERROR {}",
                response.clone().unwrap_err().to_string()
            );
        }
        let response = response.unwrap();
        if let ApiResponses::GetRequest(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    /// Allows to make a request to the node from an external Invoker
    pub async fn external_request(
        &self,
        event_request: Signed<EventRequest>,
    ) -> Result<DigestIdentifier, ApiError> {
        let response = self
            .sender
            .ask(APICommands::ExternalRequest(event_request))
            .await
            .unwrap();
        if let ApiResponses::HandleExternalRequest(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    /// It allows to obtain all the voting requests pending to be resolved in the node.
    /// These requests are received from other nodes in the network when they try to update
    /// a governance subject. It is necessary to vote their agreement or disagreement with
    /// the proposed changes in order for the events to be implemented.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurs during operation execution.
    #[cfg(feature = "approval")]
    pub async fn get_pending_requests(&self) -> Result<Vec<ApprovalEntity>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetPendingRequests)
            .await
            .unwrap();
        if let ApiResponses::GetPendingRequests(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    /// It allows to obtain a single voting request pending to be resolved in the node.
    /// This request is received from other nodes in the network when they try to update
    /// a governance subject. It is necessary to vote its agreement or disagreement with
    /// the proposed changes in order for the events to be implemented.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurs during operation execution.
    /// • [ApiError::NotFound] if the requested request does not exist.
    #[cfg(feature = "approval")]
    pub async fn get_single_request(
        &self,
        id: DigestIdentifier,
    ) -> Result<ApprovalEntity, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetSingleRequest(id))
            .await
            .unwrap();
        if let ApiResponses::GetSingleRequest(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    /// Allows to get all subjects that are known to the current node, regardless of their governance.
    /// Paging can be performed using the optional arguments `from` and `quantity`.
    /// Regarding the first one, note that it admits negative values, in which case the paging is
    /// performed in the opposite direction starting from the end of the collection. Note that this method
    /// also returns the subjects that model governance.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurred during the execution of the operation.
    pub async fn get_subjects(
        &self,
        namespace: String,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetSubjects(super::GetSubjects {
                namespace,
                from,
                quantity,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetSubjects(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    pub async fn get_subjects_by_governance(
        &self,
        governance_id: DigestIdentifier,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetSubjectByGovernance(
                super::GetSubjects {
                    namespace: "".into(),
                    from,
                    quantity,
                },
                governance_id,
            ))
            .await
            .unwrap();
        if let ApiResponses::GetSubjectByGovernance(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    /// It allows to obtain all the subjects that model existing governance in the node.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurred during the execution of the operation.
    pub async fn get_governances(
        &self,
        namespace: String,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetGovernances(super::GetSubjects {
                namespace,
                from,
                quantity,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetGovernances(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    pub async fn get_event(
        &self,
        subject_id: DigestIdentifier,
        sn: u64,
    ) -> Result<Signed<Event>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetEvent(subject_id, sn))
            .await
            .unwrap();
        if let ApiResponses::GetEvent(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    /// Allows to obtain events from a specific subject previously existing in the node.
    /// Paging can be performed by means of the optional arguments `from` and `quantity`.
    /// Regarding the former, it should be noted that negative values are allowed, in which case
    /// the paging is performed in the opposite direction starting from the end of the string.
    /// # Possible errors
    /// • [ApiError::InvalidParameters] if the specified subject identifier does not match a valid [DigestIdentifier].
    pub async fn get_events(
        &self,
        subject_id: DigestIdentifier,
        from: Option<i64>,
        quantity: Option<i64>,
    ) -> Result<Vec<Signed<Event>>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetEvents(GetEvents {
                subject_id,
                from,
                quantity,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetEvents(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    /// Allows to obtain a specified subject by specifying its identifier.
    /// # Possible errors
    /// • [ApiError::InvalidParameters] if the specified identifier does not match a valid [DigestIdentifier].<br />
    /// • [ApiError::NotFound] if the subject does not exist.
    pub async fn get_subject(&self, subject_id: DigestIdentifier) -> Result<SubjectData, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetSubject(super::GetSubject { subject_id }))
            .await
            .unwrap();
        if let ApiResponses::GetSubject(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    /// Allows to vote on a voting request that previously exists in the system.
    /// This vote will be sent to the corresponding node in charge of its collection.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurs during operation execution.<br />
    /// • [ApiError::NotFound] if the request does not exist in the system.<br />
    /// • [ApiError::InvalidParameters] if the specified request identifier does not match a valid [DigestIdentifier].<br />
    /// • [ApiError::VoteNotNeeded] if the node's vote is no longer required. <br />
    /// This occurs when the acceptance of the changes proposed by the petition has already been resolved by the rest of
    /// the nodes in the network or when the node cannot participate in the voting process because it lacks the voting role.
    #[cfg(feature = "approval")]
    pub async fn approval_request(
        &self,
        request_id: DigestIdentifier,
        acceptance: bool,
    ) -> Result<ApprovalEntity, ApiError> {
        let response = self
            .sender
            .ask(APICommands::VoteResolve(acceptance, request_id))
            .await
            .unwrap();
        if let ApiResponses::VoteResolve(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    pub async fn add_preauthorize_subject(
        &self,
        subject_id: &DigestIdentifier,
        providers: &HashSet<KeyIdentifier>,
    ) -> Result<(), ApiError> {
        let response = self
            .sender
            .ask(APICommands::SetPreauthorizedSubject(
                subject_id.clone(),
                providers.clone(),
            ))
            .await
            .unwrap();
        if let ApiResponses::SetPreauthorizedSubjectCompleted = response {
            Ok(())
        } else {
            unreachable!()
        }
    }

    pub async fn get_all_allowed_subjects_and_providers(
        &self,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<(DigestIdentifier, HashSet<KeyIdentifier>)>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetAllPreauthorizedSubjects(
                GetAllowedSubjects { from, quantity },
            ))
            .await
            .unwrap();
        if let ApiResponses::GetAllPreauthorizedSubjects(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    pub async fn add_keys(&self, derivator: KeyDerivator) -> Result<KeyIdentifier, ApiError> {
        let response = self
            .sender
            .ask(APICommands::AddKeys(derivator))
            .await
            .unwrap();
        if let ApiResponses::AddKeys(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    pub async fn get_validation_proof(
        &self,
        subject_id: DigestIdentifier,
    ) -> Result<(HashSet<Signature>, ValidationProof), ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetValidationProof(subject_id))
            .await
            .unwrap();
        if let ApiResponses::GetValidationProof(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    pub async fn get_governance_subjects(
        &self,
        governance_id: DigestIdentifier,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetGovernanceSubjects(GetGovernanceSubjects {
                governance_id,
                from,
                quantity,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetGovernanceSubjects(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    #[cfg(feature = "approval")]
    pub async fn get_approval(
        &self,
        request_id: DigestIdentifier,
    ) -> Result<ApprovalEntity, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetApproval(request_id))
            .await
            .unwrap();
        if let ApiResponses::GetApproval(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    #[cfg(feature = "approval")]
    pub async fn get_approvals(
        &self,
        state: Option<crate::ApprovalState>,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<ApprovalEntity>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetApprovals(super::GetApprovals {
                state,
                from,
                quantity,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetApprovals(data) = response {
            data
        } else {
            unreachable!()
        }
    }
}

pub struct ApiManager<C: DatabaseCollection> {
    input: MpscChannel<APICommands, ApiResponses>,
    inner_api: InnerApi<C>,
    token: CancellationToken,
    notification_tx: tokio::sync::mpsc::Sender<Notification>,
}

impl<C: DatabaseCollection> ApiManager<C> {
    pub fn new(
        input: MpscChannel<APICommands, ApiResponses>,
        event_api: EventAPI,
        #[cfg(feature = "approval")] approval_api: ApprovalAPI,
        authorized_subjects_api: AuthorizedSubjectsAPI,
        ledger_api: EventManagerAPI,
        token: CancellationToken,
        notification_tx: tokio::sync::mpsc::Sender<Notification>,
        db: DB<C>,
    ) -> Self {
        Self {
            input,
            inner_api: InnerApi::new(
                event_api,
                authorized_subjects_api,
                db,
                #[cfg(feature = "approval")]
                approval_api,
                ledger_api,
            ),
            token,
            notification_tx,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                command = self.input.receive() => {
                    if command.is_some() {
                        let result = self.process_command(command.unwrap()).await;
                        if result.is_err() {
                            error!("API error detected");
                            self.token.cancel();
                            break;
                        }
                    }
                },
                _ = self.token.cancelled() => {
                    info!("API module shutdown received");
                    break;
                }
            }
        }
        info!("Ended");
    }

    async fn process_command(
        &mut self,
        input: ChannelData<APICommands, ApiResponses>,
    ) -> Result<(), APIInternalError> {
        // TODO: API commands to change the configuration are missing
        match input {
            ChannelData::AskData(data) => {
                let (sx, command) = data.get();
                let response = match command {
                    APICommands::GetSubjects(data) => self.inner_api.get_all_subjects(data),
                    APICommands::GetGovernances(data) => {
                        self.inner_api.get_all_governances(data).await
                    }
                    APICommands::GetEvents(data) => {
                        self.inner_api.get_events_of_subject(data).await
                    }
                    APICommands::GetSubject(data) => self.inner_api.get_single_subject(data).await,
                    APICommands::GetRequest(request_id) => {
                        self.inner_api.get_request(request_id).await
                    }
                    APICommands::GetEvent(subject_id, sn) => {
                        self.inner_api.get_event(subject_id, sn)
                    }
                    #[cfg(feature = "approval")]
                    APICommands::VoteResolve(acceptance, id) => {
                        self.inner_api.emit_vote(id, acceptance).await?
                    }
                    #[cfg(feature = "approval")]
                    APICommands::GetPendingRequests => self.inner_api.get_pending_request().await,
                    #[cfg(feature = "approval")]
                    APICommands::GetSingleRequest(data) => {
                        self.inner_api.get_single_request(data).await
                    }
                    APICommands::ExternalRequest(event_request) => {
                        let response = self.inner_api.handle_external_request(event_request).await;
                        response?
                    }
                    APICommands::SetPreauthorizedSubject(subject_id, providers) => {
                        self.inner_api
                            .set_preauthorized_subject(subject_id, providers)
                            .await?
                    }
                    APICommands::AddKeys(derivator) => {
                        self.inner_api.generate_keys(derivator).await?
                    }
                    APICommands::GetValidationProof(subject_id) => {
                        self.inner_api.get_validation_proof(subject_id).await
                    }
                    APICommands::GetGovernanceSubjects(data) => {
                        self.inner_api.get_governance_subjects(data).await
                    }
                    #[cfg(feature = "approval")]
                    APICommands::GetApproval(request_id) => {
                        self.inner_api.get_approval(request_id).await
                    }
                    #[cfg(feature = "approval")]
                    APICommands::GetApprovals(get_approvals) => {
                        self.inner_api
                            .get_approvals(
                                get_approvals.state,
                                get_approvals.from,
                                get_approvals.quantity,
                            )
                            .await
                    }
                    APICommands::GetAllPreauthorizedSubjects(data) => {
                        self.inner_api
                            .get_all_preauthorized_subjects_and_providers(data)
                            .await?
                    }
                    APICommands::GetSubjectByGovernance(params, gov_id) => {
                        self.inner_api.get_subjects_by_governance(params, gov_id)
                    }
                };
                sx.send(response)
                    .map_err(|_| APIInternalError::OneshotUnavailable)?;
            }
            ChannelData::TellData(_data) => {
                panic!("Tell in API")
            }
        }
        Ok(())
    }
}
