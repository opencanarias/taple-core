use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    commons::models::{event::ValidationProof, notary::NotaryEventResponse},
    signature::Signature,
};

use self::errors::NotaryError;

pub mod errors;
#[cfg(feature = "validation")]
pub mod manager;
#[cfg(feature = "validation")]
pub mod notary;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotaryCommand {
    NotaryEvent(NotaryEvent),
}

#[derive(Debug, Clone)]
pub enum NotaryResponse {
    NotaryEventResponse(Result<NotaryEventResponse, NotaryError>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotaryEvent {
    pub proof: ValidationProof,
    pub subject_signature: Signature,
    pub previous_proof: Option<ValidationProof>,
    pub prev_event_validation_signatures: HashSet<Signature>,
}
