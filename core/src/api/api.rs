use std::collections::HashSet;

use super::{GetEventsOfSubject, GetGovernanceSubjects};
use super::{
    error::{APIInternalError, ApiError},
    inner_api::InnerAPI,
    APICommands, ApiResponses,
};
#[cfg(feature = "aproval")]
use crate::approval::manager::ApprovalAPI;
use crate::authorized_subjecs::manager::AuthorizedSubjectsAPI;
use crate::commons::models::event::Event;
use crate::commons::models::event_request::EventRequest;
use crate::commons::models::request::TapleRequest;
use crate::commons::models::state::SubjectData;
use crate::commons::{
    channel::{ChannelData, MpscChannel, SenderEnd},
    config::TapleSettings,
    crypto::KeyPair,
};
use crate::event::manager::EventAPI;
use crate::ledger::manager::EventManagerAPI;
use crate::signature::Signature;
use crate::KeyIdentifier;
use crate::{
    approval::ApprovalPetitionData,
    commons::models::Acceptance,
    event_request::{CreateRequest, EventRequestType, FactRequest},
    identifier::DigestIdentifier,
    DatabaseCollection, DB,
};
use async_trait::async_trait;
use tokio::sync::watch::Sender;

/// Trait that allows implementing the interface of a TAPLE node.
/// The only native implementation is [NodeAPI]. Users can use the trait
/// to add specific behaviors to an existing node interface. For example,
/// a [NodeAPI] wrapper could be created that again implements the trait
/// and perform certain intermediate operations, such as incrementing a counter
/// to find out how many API queries have been made.
#[async_trait]
pub trait ApiModuleInterface {
    /// Allows to generate a voting request in the system.
    /// This request will be sent to the node that will be in charge of handling
    /// the votes of the rest of the nodes belonging to the same governance.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurs during operation execution.<br />
    /// • [ApiError::InvalidParameters] if the specified request identifier does not match a valid [DigestIdentifier].<br />
    async fn create_request(
        &self,
        request_type: EventRequestType,
    ) -> Result<DigestIdentifier, ApiError>;
    /// Allows to make a request to the node from an external Invoker
    async fn external_request(
        &self,
        event_request: EventRequest,
    ) -> Result<DigestIdentifier, ApiError>;
    /// Allows adding a new event to the chain of a subject previously existing
    /// in the node. The event identifier and its [payload](RequestPayload) are required.
    /// This method returns the enumerated [CreateRequestResponse], being the identifier
    /// of a request when the event intends to update a governance, in which case
    /// it will be communicated to the rest of the nodes aware of the governance with
    /// the intention that they vote the change contained in the request. In case the event
    /// is from a conventional subject, the system returns the created event.
    /// # Possible errors
    /// This method will return [ApiError::EventCreationError] if it has not been possible
    /// to create the event, while [ApiError::InternalError] will be obtained if an internal error
    /// has occurred during the management of the operation. If it has not been possible to create
    /// the signature accompanying the creation of the event with the node identity,
    /// then [ApiError::SignError] will be obtained.
    async fn create_event(
        &self,
        subject_id: DigestIdentifier,
        payload: String,
    ) -> Result<DigestIdentifier, ApiError>;
    /// Allows to get all subjects that are known to the current node, regardless of their governance.
    /// Paging can be performed using the optional arguments `from` and `quantity`.
    /// Regarding the first one, note that it admits negative values, in which case the paging is
    /// performed in the opposite direction starting from the end of the collection. Note that this method
    /// also returns the subjects that model governance.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurred during the execution of the operation.
    async fn get_all_subjects(
        &self,
        namespace: String,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError>;
    /// It allows to obtain all the subjects that model existing governance in the node.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurred during the execution of the operation.
    async fn get_all_governances(
        &self,
        namespace: String,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError>;
    /// Allows to obtain events from a specific subject previously existing in the node.
    /// Paging can be performed by means of the optional arguments `from` and `quantity`.
    /// Regarding the former, it should be noted that negative values are allowed, in which case
    /// the paging is performed in the opposite direction starting from the end of the string.
    /// # Possible errors
    /// • [ApiError::InvalidParameters] if the specified subject identifier does not match a valid [DigestIdentifier].
    async fn get_event_of_subject(
        &self,
        subject_id: DigestIdentifier,
        from: Option<i64>,
        quantity: Option<i64>,
    ) -> Result<Vec<Event>, ApiError>;
    /// Allows to create a new subject in the node, being its owner the node in question.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurred during the execution of the operation.<br />
    /// • [ApiError::SignError] if it has not been possible to create the signature that accompanies
    /// the creation of the event with the identity of the node.<br />
    /// • [ApiError::EventCreationError] if it has not been possible to create the subject,
    /// for example, because its governance does not exist.<br />
    /// • [ApiError::InvalidParameters] if the specified governance identifier does not match a valid [DigestIdentifier].
    async fn create_subject(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
        namespace: String,
    ) -> Result<DigestIdentifier, ApiError>;
    /// Allows to obtain a specified subject by specifying its identifier.
    /// # Possible errors
    /// • [ApiError::InvalidParameters] if the specified identifier does not match a valid [DigestIdentifier].<br />
    /// • [ApiError::NotFound] if the subject does not exist.
    async fn get_subject(&self, subject_id: DigestIdentifier) -> Result<SubjectData, ApiError>;
    /// Method for creating governance in the system.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurred during the execution of the operation.<br />
    /// • [ApiError::SignError] if it has not been possible to create the signature that accompanies
    /// the creation of the event with the identity of the node.<br />
    /// • [ApiError::EventCreationError] if it has not been possible to create the governance.
    async fn create_governance(&self) -> Result<DigestIdentifier, ApiError>;
    /// Stops the node, consuming the instance on the fly. This implies that any previously created API
    /// or [NotificationHandler] instances will no longer be functional.
    async fn shutdown(self) -> Result<(), ApiError>;
    /// Allows to vote on a voting request that previously exists in the system.
    /// This vote will be sent to the corresponding node in charge of its collection.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurs during operation execution.<br />
    /// • [ApiError::NotFound] if the request does not exist in the system.<br />
    /// • [ApiError::InvalidParameters] if the specified request identifier does not match a valid [DigestIdentifier].<br />
    /// • [ApiError::VoteNotNeeded] if the node's vote is no longer required. <br />
    /// This occurs when the acceptance of the changes proposed by the petition has already been resolved by the rest of the nodes in the network or when the node cannot participate in the voting process because it lacks the voting role.
    #[cfg(feature = "aproval")]
    async fn approval_request(
        &self,
        request_id: DigestIdentifier,
        acceptance: Acceptance,
    ) -> Result<DigestIdentifier, ApiError>;
    /// It allows to obtain all the voting requests pending to be resolved in the node.
    /// These requests are received from other nodes in the network when they try to update
    /// a governance subject. It is necessary to vote their agreement or disagreement with
    /// the proposed changes in order for the events to be implemented.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurs during operation execution.
    #[cfg(feature = "aproval")]
    async fn get_pending_requests(&self) -> Result<Vec<ApprovalPetitionData>, ApiError>;
    /// It allows to obtain a single voting request pending to be resolved in the node.
    /// This request is received from other nodes in the network when they try to update
    /// a governance subject. It is necessary to vote its agreement or disagreement with
    /// the proposed changes in order for the events to be implemented.
    /// # Possible errors
    /// • [ApiError::InternalError] if an internal error occurs during operation execution.
    /// • [ApiError::NotFound] if the requested request does not exist.
    #[cfg(feature = "aproval")]
    async fn get_single_request(
        &self,
        id: DigestIdentifier,
    ) -> Result<ApprovalPetitionData, ApiError>;
    async fn add_preauthorize_subject(
        &self,
        subject_id: &DigestIdentifier,
        providers: &HashSet<KeyIdentifier>,
    ) -> Result<(), ApiError>;
    async fn generate_keys(&self) -> Result<KeyIdentifier, ApiError>;
    async fn get_validation_proof(
        &self,
        subject_id: DigestIdentifier,
    ) -> Result<HashSet<Signature>, ApiError>;
    async fn get_request(&self, request_id: DigestIdentifier) -> Result<TapleRequest, ApiError>;
    async fn get_governance_subjects(
        &self,
        governance_id: DigestIdentifier,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError>;
}

/// Object that allows interaction with a TAPLE node.
///
/// It has methods to perform all available read and write operations,
/// as well as an additional action to stop a running node.
/// he interaction is performed thanks to the implementation of a trait
/// known as [ApiModuleInterface]. Consequently, it is necessary to import
/// the trait in order to properly use the object.
#[derive(Clone, Debug)]
pub struct NodeAPI {
    pub(crate) sender: SenderEnd<APICommands, ApiResponses>,
}

/// Feature that allows implementing the API Rest of an Taple node.
#[async_trait]
impl ApiModuleInterface for NodeAPI {
    async fn create_request(
        &self,
        request_type: EventRequestType,
    ) -> Result<DigestIdentifier, ApiError> {
        let response = self
            .sender
            .ask(APICommands::HandleRequest(request_type))
            .await
            .unwrap();
        if let ApiResponses::HandleRequest(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    async fn get_request(&self, request_id: DigestIdentifier) -> Result<TapleRequest, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetRequest(request_id))
            .await
            .unwrap();
        if let ApiResponses::GetRequest(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    async fn external_request(
        &self,
        event_request: EventRequest,
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

    #[cfg(feature = "aproval")]
    async fn get_pending_requests(&self) -> Result<Vec<ApprovalPetitionData>, ApiError> {
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

    #[cfg(feature = "aproval")]
    async fn get_single_request(
        &self,
        id: DigestIdentifier,
    ) -> Result<ApprovalPetitionData, ApiError> {
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

    async fn create_event(
        &self,
        subject_id: DigestIdentifier,
        payload: String,
    ) -> Result<DigestIdentifier, ApiError> {
        let request = EventRequestType::Fact(FactRequest {
            subject_id,
            payload: payload,
        });
        let response = self
            .sender
            .ask(APICommands::HandleRequest(request))
            .await
            .unwrap();
        if let ApiResponses::HandleRequest(data) = response {
            data
        } else {
            unreachable!()
        }
    }
    async fn get_all_subjects(
        &self,
        namespace: String,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetAllSubjects(super::GetAllSubjects {
                namespace,
                from,
                quantity,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetAllSubjects(data) = response {
            data
        } else {
            unreachable!()
        }
    }
    async fn get_all_governances(
        &self,
        namespace: String,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetAllGovernances(super::GetAllSubjects {
                namespace,
                from,
                quantity,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetAllGovernances(data) = response {
            data
        } else {
            unreachable!()
        }
    }
    async fn get_event_of_subject(
        &self,
        subject_id: DigestIdentifier,
        from: Option<i64>,
        quantity: Option<i64>,
    ) -> Result<Vec<Event>, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetEventsOfSubject(GetEventsOfSubject {
                subject_id,
                from,
                quantity,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetEventsOfSubject(data) = response {
            data
        } else {
            unreachable!()
        }
    }
    async fn create_subject(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
        namespace: String,
    ) -> Result<DigestIdentifier, ApiError> {
        let request = EventRequestType::Create(CreateRequest {
            governance_id,
            schema_id,
            namespace,
        });
        let response = self
            .sender
            .ask(APICommands::HandleRequest(request))
            .await
            .unwrap();
        if let ApiResponses::HandleRequest(data) = response {
            data
        } else {
            unreachable!()
        }
    }
    async fn get_subject(&self, subject_id: DigestIdentifier) -> Result<SubjectData, ApiError> {
        let response = self
            .sender
            .ask(APICommands::GetSingleSubject(super::GetSingleSubject {
                subject_id,
            }))
            .await
            .unwrap();
        if let ApiResponses::GetSingleSubject(data) = response {
            data
        } else {
            unreachable!()
        }
    }
    async fn create_governance(&self) -> Result<DigestIdentifier, ApiError> {
        let request = EventRequestType::Create(CreateRequest {
            governance_id: DigestIdentifier::default(),
            schema_id: "".into(),
            namespace: "".into(),
        });
        let response = self
            .sender
            .ask(APICommands::HandleRequest(request))
            .await
            .unwrap();
        if let ApiResponses::HandleRequest(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    #[cfg(feature = "aproval")]
    async fn approval_request(
        &self,
        request_id: DigestIdentifier,
        acceptance: Acceptance,
    ) -> Result<DigestIdentifier, ApiError> {
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

    async fn shutdown(self) -> Result<(), ApiError> {
        let response = self.sender.ask(APICommands::Shutdown).await.unwrap();
        if let ApiResponses::ShutdownCompleted = response {
            Ok(())
        } else {
            unreachable!()
        }
    }

    async fn add_preauthorize_subject(
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

    async fn generate_keys(&self) -> Result<KeyIdentifier, ApiError> {
        let response = self.sender.ask(APICommands::GenerateKeys).await.unwrap();
        if let ApiResponses::GenerateKeys(data) = response {
            data
        } else {
            unreachable!()
        }
    }

    async fn get_validation_proof(
        &self,
        subject_id: DigestIdentifier,
    ) -> Result<HashSet<Signature>, ApiError> {
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

    async fn get_governance_subjects(
        &self,
        governance_id: DigestIdentifier,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> Result<Vec<SubjectData>, ApiError> {
        let response = self.
            sender
            .ask(APICommands::GetGovernanceSubjects(GetGovernanceSubjects {
                governance_id,
                from,
                quantity
            }))
            .await
            .unwrap();
        if let ApiResponses::GetGovernanceSubjects(data) = response {
            data
        } else {
            unreachable!()
        }
    }
}

pub struct API<C: DatabaseCollection> {
    input: MpscChannel<APICommands, ApiResponses>,
    _settings_sender: Sender<TapleSettings>,
    inner_api: InnerAPI<C>,
    shutdown_sender: Option<tokio::sync::broadcast::Sender<()>>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
}

impl<C: DatabaseCollection> API<C> {
    pub fn new(
        input: MpscChannel<APICommands, ApiResponses>,
        event_api: EventAPI,
        #[cfg(feature = "aproval")] approval_api: ApprovalAPI,
        authorized_subjects_api: AuthorizedSubjectsAPI,
        ledger_api: EventManagerAPI,
        settings_sender: Sender<TapleSettings>,
        initial_settings: TapleSettings,
        keys: KeyPair,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        db: DB<C>,
    ) -> Self {
        Self {
            input,
            _settings_sender: settings_sender,
            inner_api: InnerAPI::new(
                keys,
                &initial_settings,
                event_api,
                authorized_subjects_api,
                db,
                #[cfg(feature = "aproval")]
                approval_api,
                ledger_api,
            ),
            shutdown_sender: Some(shutdown_sender),
            shutdown_receiver: shutdown_receiver,
        }
    }

    pub async fn start(mut self) {
        let mut response_channel = None;
        loop {
            tokio::select! {
                msg = self.input.receive() => {
                    let must_shutdown = if msg.is_none() {
                        // Channel closed
                        true
                    } else {
                        let result = self.process_input(msg.unwrap()).await;
                        if result.is_err() {
                            true
                        } else {
                            let response = result.unwrap();
                            if response.is_some() {
                                response_channel = response;
                                true
                            } else {
                                false
                            }
                        }
                    };
                    if must_shutdown {
                        let sender = self.shutdown_sender.take().unwrap();
                        sender.send(()).expect("Shutdown Channel Closed");
                        drop(sender);
                        _ = self.shutdown_receiver.recv().await;
                        if response_channel.is_some() {
                            let response_channel = response_channel.unwrap();
                            let _ = response_channel.send(ApiResponses::ShutdownCompleted);
                        }
                        break;
                    }
                },
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }

    async fn process_input(
        &mut self,
        input: ChannelData<APICommands, ApiResponses>,
    ) -> Result<Option<tokio::sync::oneshot::Sender<ApiResponses>>, APIInternalError> {
        // TODO: API commands to change the configuration are missing
        match input {
            ChannelData::AskData(data) => {
                let (sx, command) = data.get();
                let response = match command {
                    APICommands::Shutdown => {
                        return Ok(Some(sx));
                    }
                    APICommands::GetAllSubjects(data) => self.inner_api.get_all_subjects(data),
                    APICommands::GetAllGovernances(data) => {
                        self.inner_api.get_all_governances(data).await
                    }
                    APICommands::GetEventsOfSubject(data) => {
                        self.inner_api.get_events_of_subject(data).await
                    }
                    APICommands::GetSingleSubject(data) => {
                        self.inner_api.get_single_subject(data).await
                    }
                    APICommands::GetRequest(request_id) => {
                        self.inner_api.get_request(request_id).await
                    }
                    #[cfg(feature = "aproval")]
                    APICommands::VoteResolve(acceptance, id) => {
                        self.inner_api.emit_vote(id, acceptance).await?
                    }
                    #[cfg(feature = "aproval")]
                    APICommands::GetPendingRequests => self.inner_api.get_pending_request().await,
                    #[cfg(feature = "aproval")]
                    APICommands::GetSingleRequest(data) => {
                        self.inner_api.get_single_request(data).await
                    }
                    APICommands::HandleRequest(data) => self.inner_api.handle_request(data).await?,
                    APICommands::ExternalRequest(event_request) => {
                        let response = self.inner_api.handle_external_request(event_request).await;
                        response?
                    }
                    APICommands::SetPreauthorizedSubject(subject_id, providers) => {
                        self.inner_api
                            .set_preauthorized_subject(subject_id, providers)
                            .await?
                    }
                    APICommands::GenerateKeys => self.inner_api.generate_keys().await?,
                    APICommands::GetValidationProof(subject_id) => {
                        self.inner_api.get_validation_proof(subject_id).await
                    }
                    APICommands::GetGovernanceSubjects(data) => {
                        self.inner_api.get_governance_subjects(data).await
                    }
                };
                sx.send(response)
                    .map_err(|_| APIInternalError::OneshotUnavailable)?;
            }
            ChannelData::TellData(_data) => {
                panic!("Tell in API")
            }
        }
        Ok(None)
    }
}
