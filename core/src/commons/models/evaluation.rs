use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::errors::SubjectError,
    signature::{Signature, Signed},
    DigestIdentifier, EventRequest, ValueWrapper,
};

use super::HashId;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EvaluationRequest {
    pub event_request: Signed<EventRequest>,
    pub context: SubjectContext,
    pub sn: u64,
    pub governance_version: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EvaluationResponse {
    pub patch: ValueWrapper, // cambiar
    pub evaluation_req_hash: DigestIdentifier,
    pub state_hash: DigestIdentifier,
    pub evaluation_success: bool, // Se ejecutó con exito y se validó el resultado contra el esquema
    pub approval_required: bool,
}

impl HashId for EvaluationResponse {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&(
            &self.evaluation_req_hash,
            self.state_hash,
            self.evaluation_success,
            self.approval_required,
        ))
        .map_err(|_| {
            SubjectError::SignatureCreationFails("HashId for EvaluationResponse Fails".to_string())
        })
    }
}

impl HashId for EvaluationRequest {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self).map_err(|_| {
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
