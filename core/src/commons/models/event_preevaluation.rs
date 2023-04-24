//! Contains the data structures related to event preevaluations to send to evaluators.
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    event_request::EventRequest,
    identifier::{DigestIdentifier, KeyIdentifier},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::state::SubjectData;

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
    pub creator: KeyIdentifier,
    #[schema(value_type = String)]
    pub owner: KeyIdentifier,
    pub actual_state: String,
    pub namespace: String,
    pub governance_version: u64,
}

impl EventPreEvaluation {
    pub fn new(
        event_request: EventRequest,
        subject_data: SubjectData,
        sn: u64,
        governance_version: u64,
    ) -> Self {
        EventPreEvaluation {
            event_request,
            context: Context {
                governance_id: subject_data.governance_id,
                schema_id: subject_data.schema_id,
                creator: subject_data.creator,
                owner: subject_data.owner,
                actual_state: subject_data.properties,
                namespace: subject_data.namespace,
                governance_version,
            },
            sn,
        }
    }
}
