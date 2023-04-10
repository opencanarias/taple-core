//! Contains the data structures related to the content of TAPLE events.
use borsh::{BorshDeserialize, BorshSerialize};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::identifier::{DigestIdentifier, KeyIdentifier};

use super::event_request::EventRequest;
/// Metadata of a TAPLE Event
#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct Metadata {
    pub namespace: String,
    #[schema(value_type = String)]
    pub subject_id: DigestIdentifier,
    #[schema(value_type = String)]
    pub governance_id: DigestIdentifier,
    pub governance_version: u64,
    pub schema_id: String,
    #[schema(value_type = String)]
    pub owner: KeyIdentifier,
}

/// Content of a TAPLE event
#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct EventContent {
    #[schema(value_type = String)]
    pub subject_id: DigestIdentifier,
    pub event_request: EventRequest,
    pub sn: u64,
    #[schema(value_type = String)]
    pub previous_hash: DigestIdentifier,
    #[schema(value_type = String)]
    pub state_hash: DigestIdentifier,
    pub metadata: Metadata,
    pub approved: bool,
}

impl EventContent {
    pub fn new(
        subject_id: DigestIdentifier,
        event_request: EventRequest,
        sn: u64,
        previous_hash: DigestIdentifier,
        metadata: Metadata,
        approved: bool,
    ) -> Self {
        EventContent {
            subject_id,
            event_request,
            sn,
            previous_hash,
            state_hash: DigestIdentifier::default(),
            metadata,
            approved,
        }
    }
}
