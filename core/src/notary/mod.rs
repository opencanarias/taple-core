use crate::{
    identifier::{DigestIdentifier, KeyIdentifier},
    signature::Signature, commons::models::notary::NotaryEventResponse,
};

use self::errors::NotaryError;

pub mod errors;
pub mod manager;
pub mod notary;

#[derive(Debug, Clone)]
pub enum NotaryCommand {
    NotaryEvent(NotaryEvent),
}

#[derive(Debug, Clone)]
pub enum NotaryResponse {
    NotaryEventResponse(Result<NotaryEventResponse, NotaryError>),
}

#[derive(Debug, Clone)]
pub struct NotaryEvent {
    pub gov_id: DigestIdentifier,
    pub subject_id: DigestIdentifier,
    pub owner: KeyIdentifier,
    pub event_hash: DigestIdentifier,
    pub sn: u64,
    pub gov_version: u64,
    pub owner_signature: Signature,
}
