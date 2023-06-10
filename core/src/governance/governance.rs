use std::{collections::HashSet, marker::PhantomData};

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    commons::{
        channel::{ChannelData, MpscChannel, SenderEnd},
        identifier::{DigestIdentifier, KeyIdentifier},
        models::event_content::Metadata,
        schema_handler::{get_governance_schema, gov_models::Contract},
    },
    DatabaseCollection, DatabaseManager, DB,
};

use super::{
    error::{InternalError, RequestError},
    inner_governance::InnerGovernance,
    stage::ValidationStage,
    GovernanceMessage, GovernanceResponse, GovernanceUpdatedMessage,
};

pub struct Governance<M: DatabaseManager<C>, C: DatabaseCollection> {
    input: MpscChannel<GovernanceMessage, GovernanceResponse>,
    shutdown_sender: tokio::sync::broadcast::Sender<()>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    inner_governance: InnerGovernance<C>,
    _m: PhantomData<M>,
}

impl<M: DatabaseManager<C>, C: DatabaseCollection> Governance<M, C> {
    pub fn new(
        input: MpscChannel<GovernanceMessage, GovernanceResponse>,
        shutdown_sender: tokio::sync::broadcast::Sender<()>,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        repo_access: DB<C>,
        update_channel: tokio::sync::broadcast::Sender<GovernanceUpdatedMessage>,
    ) -> Self {
        Self {
            input,
            shutdown_sender,
            shutdown_receiver,
            inner_governance: InnerGovernance::new(
                repo_access,
                get_governance_schema(),
                update_channel,
            ),
            _m: PhantomData::default(),
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
            let (sender, data) = match data {
                ChannelData::AskData(data) => {
                    let (sender, data) = data.get();
                    (Some(sender), data)
                }
                ChannelData::TellData(data) => {
                    let data = data.get();
                    (None, data)
                }
            };
            if let Some(sender) = sender {
                match data {
                    GovernanceMessage::GetSchema {
                        governance_id,
                        schema_id,
                        governance_version,
                    } => {
                        let to_send = self.inner_governance.get_schema(
                            governance_id,
                            schema_id,
                            governance_version,
                        )?;
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
                    GovernanceMessage::GetGovernanceVersion {
                        governance_id,
                        subject_id,
                    } => {
                        let version = self
                            .inner_governance
                            .get_governance_version(subject_id, governance_id)?;
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
                    GovernanceMessage::GetInvokeInfo {
                        metadata,
                        stage,
                        invoker,
                    } => {
                        let to_send = self
                            .inner_governance
                            .get_invoke_create_info(metadata, stage, invoker)?;
                        Ok(sender
                            .send(GovernanceResponse::GetInvokeInfo(to_send))
                            .map_err(|_| InternalError::OneshotClosed)?)
                    }
                    GovernanceMessage::GetContracts {
                        governance_id,
                        governance_version,
                    } => {
                        let to_send = self
                            .inner_governance
                            .get_contracts(governance_id, governance_version)?;
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
                    _ => unreachable!(),
                }
            } else {
                match data {
                    GovernanceMessage::GovernanceUpdated {
                        governance_id,
                        governance_version,
                    } => {
                        let result = self
                            .inner_governance
                            .governance_updated(governance_id, governance_version)
                            .await?;
                        // TODO: Revisar si hace falta tratar errores aquí
                        match result {
                            Ok(_) => {}
                            Err(_) => {}
                        }
                        Ok(())
                    }
                    _ => unreachable!(),
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
        governance_version: u64,
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
        stage: ValidationStage,
        invoker: KeyIdentifier,
    ) -> Result<bool, RequestError>;

    async fn get_contracts(
        &self,
        governance_id: DigestIdentifier,
        governance_version: u64,
    ) -> Result<Vec<Contract>, RequestError>;

    async fn get_governance_version(
        &self,
        governance_id: DigestIdentifier,
        subject_id: DigestIdentifier,
    ) -> Result<u64, RequestError>;

    async fn is_governance(&self, subject_id: DigestIdentifier) -> Result<bool, RequestError>;

    async fn governance_updated(
        &self,
        governance_id: DigestIdentifier,
        governance_version: u64,
    ) -> Result<(), RequestError>;
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
        governance_version: u64,
    ) -> Result<serde_json::Value, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetSchema {
                governance_id: governance_id.clone(),
                schema_id,
                governance_version,
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
            .ask(GovernanceMessage::GetSigners {
                metadata: metadata.clone(),
                stage,
            })
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
            .ask(GovernanceMessage::GetQuorum {
                metadata: metadata.clone(),
                stage,
            })
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
        stage: ValidationStage,
        invoker: KeyIdentifier,
    ) -> Result<bool, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetInvokeInfo {
                metadata,
                stage,
                invoker,
            })
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
        governance_version: u64,
    ) -> Result<Vec<Contract>, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetContracts {
                governance_id,
                governance_version,
            })
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
        subject_id: DigestIdentifier,
    ) -> Result<u64, RequestError> {
        let response = self
            .sender
            .ask(GovernanceMessage::GetGovernanceVersion {
                governance_id,
                subject_id,
            })
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

    async fn governance_updated(
        &self,
        governance_id: DigestIdentifier,
        governance_version: u64,
    ) -> Result<(), RequestError> {
        self.sender
            .tell(GovernanceMessage::GovernanceUpdated {
                governance_id,
                governance_version,
            })
            .await
            .map_err(|_| RequestError::ChannelClosed)
    }
}
