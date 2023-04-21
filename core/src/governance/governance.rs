use std::collections::HashSet;

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    commons::{
        channel::{ChannelData, MpscChannel, SenderEnd},
        identifier::{DigestIdentifier, KeyIdentifier},
        models::{
            event::Event, event_content::Metadata, event_request::EventRequest,
            notary::NotaryEventResponse,
        },
        schema_handler::{
            get_governance_schema,
            gov_models::{Contract, Invoke},
        },
    },
    evaluator::compiler::ContractType,
    signature::Signature,
    DatabaseManager, DB,
};

use super::{
    error::{InternalError, RequestError},
    inner_governance::InnerGovernance,
    stage::ValidationStage,
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
                GovernanceMessage::GetSchema {
                    governance_id,
                    schema_id,
                } => {
                    let to_send = self.inner_governance.get_schema(governance_id, schema_id)?;
                    Ok(sender
                        .send(GovernanceResponse::GetSchema(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::GetSigners { metadata, stage } => {
                    let to_send = self.inner_governance.get_signers(metadata, stage)?;
                    Ok(sender
                        .send(GovernanceResponse::GetSigners(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::GetQuorum { metadata, stage } => {
                    let to_send = self.inner_governance.get_quorum(metadata, stage)?;
                    Ok(sender.send(GovernanceResponse::GetQuorum(to_send)).unwrap())
                }
                GovernanceMessage::GetGovernanceVersion { governance_id } => {
                    let version = self
                        .inner_governance
                        .get_governance_version(governance_id)?;
                    Ok(sender
                        .send(GovernanceResponse::GetGovernanceVersion(version))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::IsGovernance { subject_id } => {
                    let to_send = self.inner_governance.is_governance(&subject_id)?;
                    Ok(sender
                        .send(GovernanceResponse::IsGovernance(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::GetInvokeInfo { fact, metadata } => {
                    let to_send = self.inner_governance.get_invoke_info(metadata, &fact)?;
                    Ok(sender
                        .send(GovernanceResponse::GetInvokeInfo(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::GetContracts { governance_id } => {
                    let to_send = self.inner_governance.get_contracts(governance_id)?;
                    Ok(sender
                        .send(GovernanceResponse::GetContracts(to_send))
                        .map_err(|_| InternalError::OneshotClosed)?)
                }
                GovernanceMessage::GetInitState {
                    governance_id,
                    schema_id,
                    governance_version,
                } => {
                    let to_send = self.inner_governance.get_init_state(
                        governance_id,
                        schema_id,
                        governance_version,
                    )?;
                    Ok(sender
                        .send(GovernanceResponse::GetInitState(to_send))
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
    async fn get_init_state(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
        governance_version: u64,
    ) -> Result<Value, RequestError>;
    async fn get_schema(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
    ) -> Result<serde_json::Value, RequestError>;

    async fn get_signers(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<HashSet<KeyIdentifier>, RequestError>;

    async fn get_quorum(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<u32, RequestError>;

    async fn get_invoke_info(
        &self,
        metadata: Metadata,
        fact: String,
    ) -> Result<Option<Invoke>, RequestError>;

    async fn get_contracts(
        &self,
        governance_id: DigestIdentifier,
    ) -> Result<Vec<Contract>, RequestError>;

    async fn get_governance_version(
        &self,
        governance_id: DigestIdentifier,
    ) -> Result<u64, RequestError>;

    async fn is_governance(&self, subject_id: DigestIdentifier) -> Result<bool, RequestError>;
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
    async fn get_init_state(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
        governance_version: u64,
    ) -> Result<Value, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetInitState {
                governance_id,
                schema_id,
                governance_version,
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetInitState(init_state) = response {
            init_state
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_schema(
        &self,
        governance_id: DigestIdentifier,
        schema_id: String,
    ) -> Result<serde_json::Value, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetSchema {
                governance_id,
                schema_id,
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetSchema(schema) = response {
            schema
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_signers(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<HashSet<KeyIdentifier>, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetSigners { metadata, stage })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetSigners(signers) = response {
            signers
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_quorum(
        &self,
        metadata: Metadata,
        stage: ValidationStage,
    ) -> Result<u32, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetQuorum { metadata, stage })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetQuorum(quorum) = response {
            quorum
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_invoke_info(
        &self,
        metadata: Metadata,
        fact: String,
    ) -> Result<Option<Invoke>, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetInvokeInfo { metadata, fact })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetInvokeInfo(invoke_info) = response {
            invoke_info
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_contracts(
        &self,
        governance_id: DigestIdentifier,
    ) -> Result<Vec<Contract>, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetContracts { governance_id })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetContracts(contracts) = response {
            contracts
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn get_governance_version(
        &self,
        governance_id: DigestIdentifier,
    ) -> Result<u64, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetGovernanceVersion { governance_id })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::GetGovernanceVersion(version) = response {
            version
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }

    async fn is_governance(&self, subject_id: DigestIdentifier) -> Result<bool, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::IsGovernance { subject_id })
            .await
            .map_err(|_| RequestError::ChannelClosed)?;
        if let GovernanceResponse::IsGovernance(result) = response {
            result
        } else {
            Err(RequestError::UnexpectedResponse)
        }
    }
}
