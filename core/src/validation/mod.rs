use std::collections::HashSet;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::models::{validation::ValidationEventResponse, validation::ValidationProof},
    signature::Signature,
    KeyIdentifier,
};

use self::errors::ValidationError;

pub mod errors;
#[cfg(feature = "validation")]
pub mod manager;
#[cfg(feature = "validation")]
pub mod validation;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ValidationCommand {
    ValidationEvent {
        validation_event: ValidationEvent,
        sender: KeyIdentifier,
    },
    AskForValidation(ValidationEvent),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ValidationResponse {
    ValidationEventResponse(Result<ValidationEventResponse, ValidationError>),
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ValidationEvent {
    pub proof: ValidationProof,
    pub subject_signature: Signature,
    pub previous_proof: Option<ValidationProof>,
    pub prev_event_validation_signatures: HashSet<Signature>,
}
