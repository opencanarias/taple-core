use std::collections::HashSet;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::models::{validation::ValidationProof, notary::NotaryEventResponse},
    signature::Signature,
    KeyIdentifier,
};

use self::errors::NotaryError;

pub mod errors;
#[cfg(feature = "validation")]
pub mod manager;
#[cfg(feature = "validation")]
pub mod notary;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum NotaryCommand {
    NotaryEvent {
        notary_event: NotaryEvent,
        sender: KeyIdentifier,
    },
    AskForNotary(NotaryEvent),
}

#[derive(Debug, Clone)]
pub enum NotaryResponse {
    NotaryEventResponse(Result<NotaryEventResponse, NotaryError>),
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct NotaryEvent {
    pub proof: ValidationProof,
    pub subject_signature: Signature,
    pub previous_proof: Option<ValidationProof>,
    pub prev_event_validation_signatures: HashSet<Signature>,
}
