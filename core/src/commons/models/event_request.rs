//! Contains the data structures related to event requests.
use borsh::{BorshDeserialize, BorshSerialize};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    commons::{
        errors::SubjectError,
        identifier::{DigestIdentifier, KeyIdentifier},
    },
    signature::Signed,
};

use super::{signature::Signature, value_wrapper::ValueWrapper};



#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum EventRequestType {
    Create(CreationRequest),
    Fact(FactRequest),
    Transfer(TransferRequest),
    EOL(EOLRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct CreationRequest {
    pub governance_id: DigestIdentifier,
    pub schema_id: String,
    pub namespace: String,
    pub name: String,
    pub public_key: KeyIdentifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct FactRequest {
    pub subject_id: DigestIdentifier,
    pub payload: ValueWrapper,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct TransferRequest {
    pub subject_id: DigestIdentifier,
    pub public_key: KeyIdentifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EOLRequest {
    pub subject_id: DigestIdentifier,
}

impl Signed<EventRequestType> {
    pub fn new(request: EventRequestType, signature: Signature) -> Self {
        Self {
            content: request,
            signature,
        }
    }

    pub fn verify(&self) -> Result<(), SubjectError> {
        self.signature.verify(&self.content)
    }
}
