use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::errors::SubjectError,
    signature::{Signature, Signed},
    DigestIdentifier, KeyIdentifier, ValueWrapper, EventRequest,
};

use super::HashId;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EvaluationRequest {
    pub event_request: Signed<EventRequest>, 
    pub context: SubjectContext,
    pub sn: u64,
    pub governance_version: u64
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
    pub patch: String, // cambiar
    pub evaluation_req_hash: DigestIdentifier,
    pub state_hash: DigestIdentifier,
    //pub governance_version: u64, Si no devolvemos cuando no coincide, no hace falta ponerlo aqui
    pub evaluation_success: bool, //Acceptance?  Se ejecutó con exito y se validó el resultado contra el esquema
    pub approval_required: bool,
}


impl HashId for EvaluationResponse {
    fn hash_id(&self)->DigestIdentifier {
        todo!() // no incluimos el patch
    }
}

impl HashId for EvaluationRequest {
    fn hash_id(&self)->DigestIdentifier {
        todo!()
    }
}

