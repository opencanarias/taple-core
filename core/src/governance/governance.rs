use std::collections::HashSet;

use async_trait::async_trait;

use crate::{
    commons::{
        channel::{ChannelData, MpscChannel, SenderEnd},
        identifier::{DigestIdentifier, KeyIdentifier},
        models::{
            approval_signature::ApprovalResponse, event::Event, event_content::Metadata,
            event_request::EventRequest, notary::NotaryEventResponse,
        },
        schema_handler::get_governance_schema,
    },
    evaluator::compiler::ContractType,
    DatabaseManager, DB, signature::Signature,
};

use super::{
    error::{InternalError, RequestError},
    inner_governance::InnerGovernance,
    GovernanceMessage, GovernanceResponse, RequestQuorum,
};

pub struct Governance<D: DatabaseManager> {
    input: MpscChannel<GovernanceMessage, GovernanceResponse>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    inner_governance: InnerGovernance<D>,
}

impl<D: DatabaseManager> Governance<D> {
    pub fn new(
        input: MpscChannel<GovernanceMessage, GovernanceResponse>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        repo_access: DB<D>,
    ) -> Self {
        Self {
            input,
            shutdown_sender,
            shutdown_receiver,
            inner_governance: InnerGovernance::new(repo_access, get_governance_schema()),
        }
    }

    pub async fn start(&mut self) {
        loop {
            tokio::select! {
                msg = self.input.receive() => {
                    let result = self.process_input(msg).await;
                    if result.is_err() {
                        log::error!("Error at governance module {}", result.unwrap_err());
                        self.shutdown_sender.send(()).expect("Channel Closed");
                    }
                },
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }

    async fn process_input(
        &self,
        input: Option<ChannelData<GovernanceMessage, GovernanceResponse>>,
    ) -> Result<(), InternalError> {
        if let Some(data) = input {
            let (sender, message) = if let ChannelData::AskData(data) = data {
                data.get()
            } else {
                panic!("Expected AskData, but we got TellData")
            };
            match message {
                GovernanceMessage::CheckQuorum { signers, event } => {
                    let to_send = self.inner_governance.check_quorum(signers, event)?;
                    Ok(sender
                        .send(GovernanceResponse::CheckQuorumResponse(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::GetValidators { event } => {
                    let to_send = self.inner_governance.get_validators(event)?;
                    Ok(sender
                        .send(GovernanceResponse::GetValidatorsResponse(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::CheckPolicy { .. } => {
                    let to_send = self.inner_governance.check_policy()?;
                    Ok(sender
                        .send(GovernanceResponse::CheckPolicyResponse(to_send.unwrap()))
                        .unwrap())
                }
                GovernanceMessage::GetGovernanceVersion { governance_id } => {
                    let version = self
                        .inner_governance
                        .get_governance_version(governance_id)?;
                    Ok(sender
                        .send(GovernanceResponse::GetGovernanceVersionResponse(version))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::GetSchema {
                    governance_id,
                    schema_id,
                } => {
                    let to_send = self.inner_governance.get_schema(governance_id, schema_id)?;
                    Ok(sender
                        .send(GovernanceResponse::GetSchema(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::IsGovernance(subject_id) => {
                    let to_send = self.inner_governance.is_governance(&subject_id)?;
                    Ok(sender
                        .send(GovernanceResponse::IsGovernanceResponse(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::CheckQuorumRequest {
                    event_request,
                    approvals,
                } => {
                    let to_send = self
                        .inner_governance
                        .check_quorum_request(event_request, approvals)?;
                    Ok(sender
                        .send(GovernanceResponse::CheckQuorumRequestResponse(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::GetValidatorsRequest { event_request } => {
                    let to_send = self.inner_governance.get_approvers(event_request)?;
                    Ok(sender
                        .send(GovernanceResponse::GetValidatorsRequestResponse(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::CheckInvokatorPermission {
                    subject_id,
                    invokator,
                    additional_payload,
                    metadata,
                } => {
                    let to_send = self.inner_governance.check_invokation_permission(
                        subject_id,
                        invokator,
                        additional_payload,
                        metadata,
                    )?;
                    Ok(sender
                        .send(GovernanceResponse::CheckInvokatorPermissionResponse(
                            to_send,
                        ))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
            }
        } else {
            Err(InternalError::ChannelError {
                source: crate::commons::errors::ChannelErrors::ChannelClosed,
            })
        }
    }
}

#[async_trait]
pub trait GovernanceInterface: Sync + Send {
    async fn check_quorum(
        &self,
        event: Event,
        signers: &HashSet<KeyIdentifier>,
    ) -> Result<(bool, HashSet<KeyIdentifier>), RequestError>;
    async fn check_quorum_request(
        &self,
        event_request: EventRequest,
        approvals: HashSet<ApprovalResponse>,
    ) -> Result<(RequestQuorum, HashSet<KeyIdentifier>), RequestError>;
    async fn check_policy(
        &self,
        governance_id: &DigestIdentifier,
        governance_version: u64,
        schema_id: &String,
        subject_namespace: &String,
        controller_namespace: &String,
    ) -> Result<bool, RequestError>;
    async fn get_validators(&self, event: Event) -> Result<HashSet<KeyIdentifier>, RequestError>;
    async fn get_approvers(
        &self,
        event_request: EventRequest,
    ) -> Result<HashSet<KeyIdentifier>, RequestError>;
    async fn get_governance_version(
        &self,
        governance_id: &DigestIdentifier,
    ) -> Result<u64, RequestError>;
    async fn get_schema(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &String,
    ) -> Result<serde_json::Value, RequestError>;
    async fn is_governance(&self, subject_id: &DigestIdentifier) -> Result<bool, RequestError>;
    async fn check_invokation_permission(
        &self,
        subject_id: DigestIdentifier,
        invokator: KeyIdentifier,
        additional_payload: Option<String>,
        metadata: Option<Metadata>,
    ) -> Result<(bool, bool), RequestError>;
    async fn get_contracts(
        &self,
        governance_id: DigestIdentifier,
    ) -> Result<Vec<(String, ContractType)>, RequestError>;
    async fn check_if_witness(
        &self,
        governance_id: DigestIdentifier,
        namespace: String,
        schema_id: String,
    ) -> Result<bool, RequestError>;
    async fn check_notary_signatures(
        &self,
        signatures: HashSet<NotaryEventResponse>,
        data_hash: DigestIdentifier,
        governance_id: DigestIdentifier,
        namespace: String,
    ) -> Result<(), RequestError>;
    async fn check_evaluator_signatures(
        &self,
        signatures: HashSet<Signature>,
        governance_id: DigestIdentifier,
        governance_version: u64,
        namespace: String,
    )-> Result<(), RequestError>;
}

#[derive(Debug, Clone)]
pub struct GovernanceAPI {
    sender: SenderEnd<GovernanceMessage, GovernanceResponse>,
}

impl GovernanceAPI {
    pub fn new(sender: SenderEnd<GovernanceMessage, GovernanceResponse>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl GovernanceInterface for GovernanceAPI {
    async fn get_schema(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &String,
    ) -> Result<serde_json::Value, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetSchema {
                governance_id: governance_id.clone(),
                schema_id: schema_id.clone(),
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetSchema(schema) = response {
            schema
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn check_policy(
        &self,
        governance_id: &DigestIdentifier,
        governance_version: u64,
        schema_id: &String,
        subject_namespace: &String,
        controller_namespace: &String,
    ) -> Result<bool, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::CheckPolicy {
                governance_id: governance_id.clone(),
                governance_version,
                schema_id: schema_id.clone(),
                subject_namespace: subject_namespace.clone(),
                controller_namespace: controller_namespace.clone(),
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::CheckPolicyResponse(result) = response {
            Ok(result)
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_governance_version(
        &self,
        governance_id: &DigestIdentifier,
    ) -> Result<u64, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetGovernanceVersion {
                governance_id: governance_id.clone(),
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetGovernanceVersionResponse(version) = response {
            version
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_validators(&self, event: Event) -> Result<HashSet<KeyIdentifier>, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetValidators { event })
            .await
            .expect("The mockup does not fail");
        if let GovernanceResponse::GetValidatorsResponse(validators) = response {
            validators
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_approvers(
        &self,
        event_request: EventRequest,
    ) -> Result<HashSet<KeyIdentifier>, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetValidatorsRequest { event_request })
            .await
            .expect("The mockup does not fail");
        if let GovernanceResponse::GetValidatorsRequestResponse(validators) = response {
            validators
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn check_quorum(
        &self,
        event: Event,
        signers: &HashSet<KeyIdentifier>,
    ) -> Result<(bool, HashSet<KeyIdentifier>), RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::CheckQuorum {
                event,
                signers: signers.clone(),
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::CheckQuorumResponse(result) = response {
            result
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn check_quorum_request(
        &self,
        event_request: EventRequest,
        approvals: HashSet<ApprovalResponse>,
    ) -> Result<(RequestQuorum, HashSet<KeyIdentifier>), RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::CheckQuorumRequest {
                event_request,
                approvals,
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::CheckQuorumRequestResponse(result) = response {
            result
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn is_governance(&self, subject_id: &DigestIdentifier) -> Result<bool, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::IsGovernance(subject_id.clone()))
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::IsGovernanceResponse(is_governance) = response {
            is_governance
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn check_invokation_permission(
        &self,
        subject_id: DigestIdentifier,
        invokator: KeyIdentifier,
        additional_payload: Option<String>,
        metadata: Option<Metadata>,
    ) -> Result<(bool, bool), RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::CheckInvokatorPermission {
                subject_id,
                invokator,
                additional_payload,
                metadata,
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::CheckInvokatorPermissionResponse(invokation_permission) =
            response
        {
            invokation_permission
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_contracts(
        &self,
        governance_id: DigestIdentifier,
    ) -> Result<Vec<(String, ContractType)>, RequestError> {
        todo!()
    }

    async fn check_if_witness(
        &self,
        governance_id: DigestIdentifier,
        namespace: String,
        schema_id: String
    ) -> Result<bool, RequestError> {
        todo!()
    }

    async fn check_notary_signatures(
        &self,
        signatures: HashSet<NotaryEventResponse>,
        data_hash: DigestIdentifier,
        governance_id: DigestIdentifier,
        namespace: String,
    ) -> Result<(), RequestError> {
        todo!()
    }

    async fn check_evaluator_signatures(
        &self,
        signatures: HashSet<Signature>,
        governance_id: DigestIdentifier,
        governance_version: u64,
        namespace: String,
    )-> Result<(), RequestError> {
        todo!()
    }
}
