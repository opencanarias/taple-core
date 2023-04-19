//! Contains the data structures related to event preevaluations to send to evaluators.
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    event_request::EventRequest,
    identifier::{DigestIdentifier, KeyIdentifier},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct EventPreEvaluation {
    pub event_request: EventRequest,
    pub context: Context,
    pub sn: u64,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct Context {
    #[schema(value_type = String)]
    pub governance_id: DigestIdentifier,
    pub schema_id: String,
    #[schema(value_type = String)]
    pub invokator: KeyIdentifier,
    #[schema(value_type = String)]
    pub creator: KeyIdentifier,
    #[schema(value_type = String)]
    pub owner: KeyIdentifier,
    pub actual_state: String,
    pub namespace: String,
}

impl EventPreEvaluation {
    pub fn new(
        event_request: EventRequest,
        context: Context,
        sn: u64,
    ) -> Self {
        EventPreEvaluation {
            event_request,
            context,
            sn,
        }
    }
}
