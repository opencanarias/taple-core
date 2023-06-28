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
    ApprovalResponse, Derivable, EvaluationRequest, EvaluationResponse,
};
use borsh::{BorshDeserialize, BorshSerialize};
use json_patch::diff;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    approval::ApprovalRequest,
    evaluation::SubjectContext,
    state::{generate_subject_id, Subject},
    value_wrapper::ValueWrapper,
    HashId,
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
    pub eval_success: bool, //Acceptance?  Se ejecutó con exito y se validó el resultado contra el esquema. Si no se evalua es true
    pub appr_required: bool, // no puede ser true si evaluation_success = false
    pub approved: bool,     // por defecto true, si approval_required = false
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

impl HashId for Signed<Event> {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self).map_err(|_| {
            SubjectError::SignatureCreationFails("HashId for Signed Event Fails".to_string())
        })
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
            eval_success: true,
            appr_required: false,
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

    pub fn verify_signatures(&self) -> Result<(), SubjectError> {
        // Verify event and event_request signatures
        self.signature.verify(&self.content)?;
        self.content.event_request.verify()
    }

    pub fn verify_eval_appr(
        &self,
        subject_context: SubjectContext,
        eval_sign_info: (&HashSet<KeyIdentifier>, u32, u32),
        appr_sign_info: (&HashSet<KeyIdentifier>, u32, u32),
    ) -> Result<(), SubjectError> {
        if !self.content.event_request.content.requires_eval_appr()
            && self.content.eval_success
            && self.content.approved
            && !self.content.appr_required
        {
            return Ok(());
        }
        // Verify evaluators signatures
        let eval_request = EvaluationRequest {
            event_request: self.content.event_request.clone(),
            context: subject_context,
            sn: self.content.sn,
            gov_version: self.content.gov_version,
        };
        let eval_response = EvaluationResponse {
            patch: self.content.patch.clone(), // Esto no hace falta realmente
            eval_req_hash: eval_request.hash_id()?,
            state_hash: self.content.state_hash.clone(),
            eval_success: self.content.eval_success,
            appr_required: self.content.appr_required,
        };
        let mut evaluators = HashSet::new();
        for eval_signature in self.content.evaluators.iter() {
            if !evaluators.insert(eval_signature.signer.clone()) {
                return Err(SubjectError::RepeatedSignature(
                    "Repeated Signer in Evaluators".to_string(),
                ));
            }
            eval_signature.verify(&eval_response)?;
        }
        if !evaluators.is_subset(eval_sign_info.0) {
            return Err(SubjectError::SignersError(
                "Incorrect Evaluators signed".to_string(),
            ));
        }
        let quorum_size_eval = if self.content.eval_success {
            eval_sign_info.1
        } else {
            eval_sign_info.2
        };
        if evaluators.len() < quorum_size_eval as usize {
            return Err(SubjectError::SignersError(
                "Not enough Evaluators signed".to_string(),
            ));
        }
        if self.content.approved && !self.content.appr_required {
            return Ok(());
        }
        // Verify approvers signatures
        let appr_request = ApprovalRequest {
            event_request: self.content.event_request.clone(),
            sn: self.content.sn,
            gov_version: self.content.gov_version,
            patch: self.content.patch.clone(),
            state_hash: self.content.state_hash.clone(),
            hash_prev_event: self.content.hash_prev_event.clone(),
        };
        let appr_response = ApprovalResponse {
            appr_req_hash: appr_request.hash_id()?,
            approved: self.content.approved,
        };
        let mut approvers = HashSet::new();
        for appr_signature in self.content.approvers.iter() {
            if !approvers.insert(appr_signature.signer.clone()) {
                return Err(SubjectError::RepeatedSignature(
                    "Repeated Signer in Approvers".to_string(),
                ));
            }
            appr_signature.verify(&appr_response)?;
        }
        if !approvers.is_subset(appr_sign_info.0) {
            return Err(SubjectError::SignersError(
                "Incorrect Approvers signed".to_string(),
            ));
        }
        let quorum_size_appr = if self.content.approved {
            appr_sign_info.1
        } else {
            appr_sign_info.2
        };
        if approvers.len() < appr_sign_info.1 as usize {
            return Err(SubjectError::SignersError(
                "Not enough Approvers signed".to_string(),
            ));
        }
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
