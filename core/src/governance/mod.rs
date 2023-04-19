use std::collections::HashSet;

use crate::commons::{
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{
        event_content::Metadata,
    },
    schema_handler::gov_models::{Invoke, Contract},
};
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
    GetSchema {
        governance_id: DigestIdentifier,
        schema_id: String,
        // TODO: Consider the version. The Event Sourcing of the events must be taken into account.
        // governance_version: u64,
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
        fact: String,
    },
    GetContracts {
        governance_id: DigestIdentifier,
    },
    GetGovernanceVersion {
        governance_id: DigestIdentifier,
    },
    IsGovernance {
        subject_id: DigestIdentifier,
    },
}

#[derive(Debug, Clone)]
pub enum GovernanceResponse {
    GetSchema(Result<Value, RequestError>),
    GetSigners(Result<HashSet<KeyIdentifier>, RequestError>),
    GetQuorum(Result<u32, RequestError>),
    GetInvokeInfo(Result<Option<Invoke>, RequestError>),
    GetContracts(Result<Vec<Contract>, RequestError>),
    GetGovernanceVersion(Result<u64, RequestError>),
    IsGovernance(Result<bool, RequestError>),
}
