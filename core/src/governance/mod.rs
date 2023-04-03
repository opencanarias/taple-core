use std::collections::HashSet;

use crate::commons::{
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{
        approval_signature::ApprovalResponse, event::Event, event_content::Metadata,
        event_request::EventRequest,
    },
};
pub mod error;
pub mod governance;
pub mod inner_governance;

pub use governance::{GovernanceAPI, GovernanceInterface};

use error::RequestError;
use serde_json::Value;

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
    /// ValidateEvent
    CheckQuorum {
        event: Event,
        signers: HashSet<KeyIdentifier>,
    },
    CheckQuorumRequest {
        event_request: EventRequest,
        approvals: HashSet<ApprovalResponse>,
    },
    /// GetValidatorList
    GetValidators {
        event: Event,
    },
    GetValidatorsRequest {
        event_request: EventRequest,
    },
    CheckPolicy {
        governance_id: DigestIdentifier,
        governance_version: u64,
        schema_id: String,
        subject_namespace: String,
        controller_namespace: String,
    },
    GetGovernanceVersion {
        governance_id: DigestIdentifier,
    },
    GetSchema {
        governance_id: DigestIdentifier,
        schema_id: String,
        // TODO: Consider the version. The Event Sourcing of the events must be taken into account.
        // governance_version: u64,
    },
    IsGovernance(DigestIdentifier),
    CheckInvokatorPermission {
        subject_id: DigestIdentifier,
        invokator: KeyIdentifier,
        additional_payload: Option<String>,
        metadata: Option<Metadata>,
    },
}

#[derive(Debug, Clone)]
pub enum GovernanceResponse {
    CheckQuorumResponse(Result<(bool, HashSet<KeyIdentifier>), RequestError>),
    CheckQuorumRequestResponse(Result<(RequestQuorum, HashSet<KeyIdentifier>), RequestError>),
    GetValidatorsResponse(Result<HashSet<KeyIdentifier>, RequestError>),
    GetValidatorsRequestResponse(Result<HashSet<KeyIdentifier>, RequestError>),
    CheckPolicyResponse(bool),
    GetGovernanceVersionResponse(Result<u64, RequestError>),
    GetSchema(Result<Value, RequestError>),
    IsGovernanceResponse(Result<bool, RequestError>),
    CheckInvokatorPermissionResponse(Result<(bool, bool), RequestError>),
}
