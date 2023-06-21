//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::collections::HashSet;

use crate::{
    commons::{
        crypto::{check_cryptography, KeyMaterial, KeyPair, Payload, DSA},
        errors::SubjectError,
    },
    event_content::Metadata,
    event_request::{CreateRequest, EventRequest},
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    signature::Signature,
};
use borsh::{BorshDeserialize, BorshSerialize};
use json_patch::{diff, Patch};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    approval::Approval,
    event_proposal::{EventProposal, Proposal},
    state::Subject, value_wrapper::ValueWrapper,
};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Event {
    pub content: EventContent,
    pub signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EventContent {
    pub event_proposal: EventProposal,
    pub approvals: HashSet<Approval>,
    pub execution: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct ValidationProof {
    pub subject_id: DigestIdentifier,
    pub schema_id: String,
    pub namespace: String,
    pub name: String,
    pub subject_public_key: KeyIdentifier,
    pub governance_id: DigestIdentifier,
    pub genesis_governance_version: u64,
    pub sn: u64,
    pub prev_event_hash: DigestIdentifier,
    pub event_hash: DigestIdentifier,
    pub governance_version: u64,
}

impl ValidationProof {
    pub fn new_from_genesis_event(
        create_request: CreateRequest,
        event_hash: DigestIdentifier,
        governance_version: u64,
        subject_id: DigestIdentifier,
    ) -> Self {
        Self {
            governance_id: create_request.governance_id,
            governance_version,
            subject_id,
            sn: 0,
            schema_id: create_request.schema_id,
            namespace: create_request.namespace,
            prev_event_hash: DigestIdentifier::default(),
            event_hash,
            subject_public_key: create_request.public_key,
            genesis_governance_version: governance_version,
            name: create_request.name,
        }
    }
    pub fn new_from_transfer_event(
        subject: &Subject,
        sn: u64,
        prev_event_hash: DigestIdentifier,
        event_hash: DigestIdentifier,
        governance_version: u64,
        subject_public_key: KeyIdentifier,
    ) -> Self {
        Self {
            governance_id: subject.governance_id.clone(),
            governance_version,
            subject_id: subject.subject_id.clone(),
            sn,
            schema_id: subject.schema_id.clone(),
            namespace: subject.namespace.clone(),
            prev_event_hash,
            event_hash,
            subject_public_key,
            genesis_governance_version: subject.genesis_gov_version,
            name: subject.name.clone(),
        }
    }

    pub fn new(
        subject: &Subject,
        sn: u64,
        prev_event_hash: DigestIdentifier,
        event_hash: DigestIdentifier,
        governance_version: u64,
    ) -> Self {
        Self {
            governance_id: subject.governance_id.clone(),
            governance_version,
            subject_id: subject.subject_id.clone(),
            sn,
            schema_id: subject.schema_id.clone(),
            namespace: subject.namespace.clone(),
            prev_event_hash,
            event_hash,
            subject_public_key: subject.public_key.clone(),
            genesis_governance_version: subject.genesis_gov_version,
            name: subject.name.clone(),
        }
    }

    pub fn get_metadata(&self) -> Metadata {
        Metadata {
            namespace: self.namespace.clone(),
            governance_id: self.governance_id.clone(),
            governance_version: self.governance_version,
            schema_id: self.schema_id.clone(),
            subject_id: self.subject_id.clone(),
        }
    }

    pub fn is_similar(&self, other: &ValidationProof) -> bool {
        self.governance_id == other.governance_id
            && self.subject_id == other.subject_id
            && self.sn == other.sn
            && self.schema_id == other.schema_id
            && self.namespace == other.namespace
            && self.prev_event_hash == other.prev_event_hash
            && self.event_hash == other.event_hash
            && self.subject_public_key == other.subject_public_key
            && self.genesis_governance_version == other.genesis_governance_version
            && self.name == other.name
    }
}

impl EventContent {
    pub fn new(
        event_proposal: EventProposal,
        approvals: HashSet<Approval>,
        execution: bool,
    ) -> Self {
        Self {
            event_proposal,
            approvals,
            execution,
        }
    }
}

impl Event {
    pub fn from_genesis_request(
        event_request: EventRequest,
        subject_keys: &KeyPair,
        gov_version: u64,
        init_state: &ValueWrapper,
    ) -> Result<Self, SubjectError> {
        let json_patch = serde_json::to_value(diff(&json!({}), &init_state.0)).map_err(|_| {
            SubjectError::CryptoError(String::from("Error converting patch to value"))
        })?;
        let proposal = Proposal {
            event_request,
            sn: 0,
            hash_prev_event: DigestIdentifier::default(),
            gov_version,
            evaluation: None,
            json_patch: ValueWrapper(json_patch),
            evaluation_signatures: HashSet::new(),
        };
        let public_key = KeyIdentifier::new(
            subject_keys.get_key_derivator(),
            &subject_keys.public_key_bytes(),
        );
        let subject_signature_proposal =
            Signature::new(&proposal, public_key.clone(), &subject_keys).map_err(|_| {
                SubjectError::CryptoError(String::from("Error signing the hash of the proposal"))
            })?;
        let event_proposal = EventProposal::new(proposal, subject_signature_proposal);
        let content = EventContent {
            event_proposal,
            approvals: HashSet::new(),
            execution: true,
        };
        let subject_signature_event = Signature::new(&content, public_key.clone(), &subject_keys)
            .map_err(|_| {
            SubjectError::CryptoError(String::from("Error signing the hash of the proposal"))
        })?;
        Ok(Self {
            content,
            signature: subject_signature_event,
        })
    }

    pub fn check_signatures(&self) -> Result<(), SubjectError> {
        check_cryptography(&self.content, &self.signature)
            .map_err(|error| SubjectError::CryptoError(error.to_string()))?;
        self.content.event_proposal.check_signatures()?;
        Ok(())
    }
}
