//! Contains the data structures related to event preevaluations to send to evaluators.
use borsh::{BorshDeserialize, BorshSerialize};
use serde_json::Value;

use crate::{
    event_request::EventRequest,
    identifier::{DigestIdentifier, KeyIdentifier},
};
use serde::{Deserialize, Serialize};

use super::{state::SubjectData, value_wrapper::ValueWrapper};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EventPreEvaluation {
    pub event_request: EventRequest,
    pub context: Context,
    pub sn: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Context {
    pub governance_id: DigestIdentifier,
    pub schema_id: String,
    pub creator: KeyIdentifier,
    pub owner: KeyIdentifier,
    pub actual_state: ValueWrapper,
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
