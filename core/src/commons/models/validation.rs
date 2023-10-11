//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use crate::{
    commons::{errors::SubjectError, models::event::Metadata},
    identifier::{DigestIdentifier, KeyIdentifier},
    request::StartRequest,
    signature::Signature, DigestDerivator,
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::{state::Subject, HashId};

/// A struct representing a validation proof.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct ValidationProof {
    /// The identifier of the subject being validated.
    pub subject_id: DigestIdentifier,
    /// The identifier of the schema used to validate the subject.
    pub schema_id: String,
    /// The namespace of the subject being validated.
    pub namespace: String,
    /// The name of the subject being validated.
    pub name: String,
    /// The identifier of the public key of the subject being validated.
    pub subject_public_key: KeyIdentifier,
    /// The identifier of the governance contract associated with the subject being validated.
    pub governance_id: DigestIdentifier,
    /// The version of the governance contract that created the subject being validated.
    pub genesis_governance_version: u64,
    /// The sequence number of the subject being validated.
    pub sn: u64,
    /// The identifier of the previous event in the validation chain.
    pub prev_event_hash: DigestIdentifier,
    /// The identifier of the current event in the validation chain.
    pub event_hash: DigestIdentifier,
    /// The version of the governance contract used to validate the subject.
    pub governance_version: u64,
}

impl HashId for ValidationProof {
    fn hash_id(&self,derivator: DigestDerivator) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self, derivator)
            .map_err(|_| SubjectError::CryptoError("Hashing error in ValidationProof".to_string()))
    }
}

impl ValidationProof {
    pub fn new_from_genesis_event(
        start_request: StartRequest,
        event_hash: DigestIdentifier,
        governance_version: u64,
        subject_id: DigestIdentifier,
    ) -> Self {
        Self {
            governance_id: start_request.governance_id,
            governance_version,
            subject_id,
            sn: 0,
            schema_id: start_request.schema_id,
            namespace: start_request.namespace,
            prev_event_hash: DigestIdentifier::default(),
            event_hash,
            subject_public_key: start_request.public_key,
            genesis_governance_version: governance_version,
            name: start_request.name,
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

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    PartialOrd,
)]
pub struct ValidationEventResponse {
    pub validation_signature: Signature,
    pub gov_version_validation: u64,
}
