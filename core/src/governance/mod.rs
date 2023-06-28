use std::collections::HashSet;

use crate::{commons::{
    identifier::{DigestIdentifier, KeyIdentifier},
    models::event::Metadata,
    schema_handler::gov_models::{Contract},
}, ValueWrapper};
pub mod error;
pub mod governance;
pub mod inner_governance;
pub mod stage;

pub use governance::{GovernanceAPI, GovernanceInterface};

use error::RequestError;
use serde_json::Value;

use self::stage::ValidationStage;

#[derive(Debug, Clone, PartialEq)]
pub enum RequestQuorum {
    Accepted,
    Rejected,
    Processing,
}

#[derive(Debug, Clone)]
pub struct GovernanceMember {
    pub id: String,
    pub namespace: String,
    pub description: String,
    pub key: KeyIdentifier,
}

#[derive(Debug, Clone)]
pub struct SingleGovernance {
    pub quorum: f32,
    pub members: Vec<GovernanceMember>,
    pub schemas: Vec<()>,
}

#[derive(Debug, Clone)]
pub enum GovernanceMessage {
    GetInitState {
        governance_id: DigestIdentifier,
        schema_id: String,
        governance_version: u64,
    },
    GetSchema {
        governance_id: DigestIdentifier,
        schema_id: String,
        governance_version: u64,
    },
    GetSigners {
        metadata: Metadata,
        stage: ValidationStage,
    },
    GetQuorum {
        metadata: Metadata,
        stage: ValidationStage,
    },
    GetInvokeInfo {
        metadata: Metadata,
        stage: ValidationStage,
        invoker: KeyIdentifier,
    },
    GetContracts {
        governance_id: DigestIdentifier,
        governance_version: u64,
    },
    GetGovernanceVersion {
        governance_id: DigestIdentifier,
        subject_id: DigestIdentifier
    },
    IsGovernance {
        subject_id: DigestIdentifier,
    },
    GovernanceUpdated {
        governance_id: DigestIdentifier,
        governance_version: u64,
    }}

#[derive(Debug, Clone)]
pub enum GovernanceResponse {
    GetInitState(Result<ValueWrapper, RequestError>),
    GetSchema(Result<ValueWrapper, RequestError>),
    GetSigners(Result<HashSet<KeyIdentifier>, RequestError>),
    GetQuorum(Result<u32, RequestError>),
    GetInvokeInfo(Result<bool, RequestError>),
    GetContracts(Result<Vec<(Contract, String)>, RequestError>),
    GetGovernanceVersion(Result<u64, RequestError>),
    IsGovernance(Result<bool, RequestError>),
    NoResponse,
}

#[derive(Debug, Clone)]
pub enum GovernanceUpdatedMessage {
    GovernanceUpdated {
        governance_id: DigestIdentifier,
        governance_version: u64,
    },
}
