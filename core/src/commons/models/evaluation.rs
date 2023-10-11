use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::errors::SubjectError,
    signature::{Signature, Signed},
    DigestIdentifier, EventRequest, ValueWrapper, DigestDerivator,
};

use super::HashId;

/// A struct representing an evaluation request.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EvaluationRequest {
    /// The signed event request.
    pub event_request: Signed<EventRequest>,
    /// The context in which the evaluation is being performed.
    pub context: SubjectContext,
    /// The sequence number of the event.
    pub sn: u64,
    /// The version of the governance contract.
    pub gov_version: u64,
}
// firmada por sujeto

//las cosas que no se pueden sacar del propio evento, sino que dependen del estado actual del sujeto
//lo generamos a partir del sujeto actual, no necesitamos mas nada
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct SubjectContext {
    pub governance_id: DigestIdentifier,
    pub schema_id: String,
    pub is_owner: bool,
    pub state: ValueWrapper,
    pub namespace: String,
    //pub governance_version: u64, // está en evento
}

/// A struct representing an evaluation response.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EvaluationResponse {
    /// The patch to apply to the state.
    pub patch: ValueWrapper,
    /// The hash of the evaluation request being responded to.
    pub eval_req_hash: DigestIdentifier,
    /// The hash of the state after applying the patch.
    pub state_hash: DigestIdentifier,
    /// Whether the evaluation was successful and the result was validated against the schema.
    pub eval_success: bool,
    /// Whether approval is required for the evaluation to be applied to the state.
    pub appr_required: bool,
}

impl HashId for EvaluationResponse {
    fn hash_id(&self, derivator: DigestDerivator) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&(
            &self.eval_req_hash,
            &self.state_hash,
            self.eval_success,
            self.appr_required,
        ), derivator)
        .map_err(|_| {
            SubjectError::SignatureCreationFails("HashId for EvaluationResponse Fails".to_string())
        })
    }
}

impl HashId for EvaluationRequest {
    fn hash_id(&self, derivator: DigestDerivator) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self, derivator).map_err(|_| {
            SubjectError::SignatureCreationFails("HashId for EvaluationRequest Fails".to_string())
        })
    }
}

impl Signed<EvaluationRequest> {
    pub fn new(eval_request: EvaluationRequest, signature: Signature) -> Self {
        Self {
            content: eval_request,
            signature,
        }
    }

    pub fn verify(&self) -> Result<(), SubjectError> {
        self.signature.verify(&self.content)
    }
}

impl Signed<EvaluationResponse> {
    pub fn new(eval_response: EvaluationResponse, signature: Signature) -> Self {
        Self {
            content: eval_response,
            signature,
        }
    }

    pub fn verify(&self) -> Result<(), SubjectError> {
        self.signature.verify(&self.content)
    }
}
