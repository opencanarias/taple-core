//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::collections::HashSet;

use crate::{
    commons::{
        crypto::{KeyMaterial, KeyPair},
        errors::SubjectError,
    },
    identifier::{DigestIdentifier, KeyIdentifier},
    request::{EventRequest, StartRequest},
    signature::{Signature, Signed},
    ApprovalResponse, Derivable,
};
use borsh::{BorshDeserialize, BorshSerialize};
use json_patch::diff;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    approval::ApprovalRequest,
    state::{generate_subject_id, Subject},
    value_wrapper::ValueWrapper, HashId,
};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Event {
    pub subject_id: DigestIdentifier,
    pub event_request: Signed<EventRequest>,
    pub sn: u64,
    pub gov_version: u64,
    pub patch: ValueWrapper, // cambiar
    pub state_hash: DigestIdentifier,
    // Si EventRequest
    pub evaluation_success: bool, //Acceptance?  Se ejecutó con exito y se validó el resultado contra el esquema. Si no se evalua es true
    pub approval_required: bool,  // no puede ser true si evaluation_success = false
    pub approved: bool,           // por defecto true, si approval_required = false
    pub hash_prev_event: DigestIdentifier,
    pub evaluators: HashSet<Signature>, //hace falta la firma? Hashset
    pub approvers: HashSet<Signature>,  //hace falta la firma? Hashset
}

// impl Event {
//     pub fn new(
//         event_proposal: Signed<Proposal>,
//         approvals: HashSet<Signed<ApprovalContent>>,
//         execution: bool,
//     ) -> Self {
//         Self {
//             event_proposal,
//             approvals,
//             execution,
//         }
//     }
// }
impl HashId for Event {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self)
            .map_err(|_| SubjectError::SignatureCreationFails("HashId for Event Fails".to_string()))
    }
}

impl Signed<Event> {
    pub fn from_genesis_request(
        event_request: Signed<EventRequest>,
        subject_keys: &KeyPair,
        gov_version: u64,
        init_state: &ValueWrapper,
    ) -> Result<Self, SubjectError> {
        let EventRequest::Create(start_request) = event_request.content.clone() else {
            return Err(SubjectError::NotCreateEvent)
        };
        let json_patch = serde_json::to_value(diff(&json!({}), &init_state.0)).map_err(|_| {
            SubjectError::CryptoError(String::from("Error converting patch to value"))
        })?;
        let public_key = KeyIdentifier::new(
            subject_keys.get_key_derivator(),
            &subject_keys.public_key_bytes(),
        );
        let subject_id = generate_subject_id(
            &start_request.namespace,
            &start_request.schema_id,
            public_key.to_str(),
            start_request.governance_id.to_str(),
            gov_version,
        )?;
        let state_hash = DigestIdentifier::from_serializable_borsh(init_state).map_err(|_| {
            SubjectError::CryptoError(String::from("Error converting state to hash"))
        })?;
        let content = Event {
            subject_id,
            event_request,
            sn: 0,
            gov_version,
            patch: ValueWrapper(json_patch),
            state_hash,
            evaluation_success: true,
            approval_required: false,
            approved: true,
            hash_prev_event: DigestIdentifier::default(),
            evaluators: HashSet::new(),
            approvers: HashSet::new(),
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

    pub fn verify(&self) -> Result<(), SubjectError> {
        // Verify event and event_request signatures
        self.signature.verify(&self.content)?;
        self.content.event_request.verify()?;
        // Verify evaluators signatures

        // Verify approvers signatures

        Ok(())
    }
}

/// Metadata of a TAPLE Event
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Metadata {
    pub namespace: String,
    pub subject_id: DigestIdentifier,
    pub governance_id: DigestIdentifier,
    pub governance_version: u64,
    pub schema_id: String,
}
