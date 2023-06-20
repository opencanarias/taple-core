use borsh::{BorshSerialize, BorshDeserialize};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    commons::models::{approval::Approval, event_proposal::Evaluation},
    event_request::EventRequest,
    identifier::DigestIdentifier,
    signature::Signature,
    Event, KeyIdentifier,
};

use self::errors::EventError;

pub mod errors;
pub mod event_completer;
pub mod manager;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum EventCommand {
    Event {
        event_request: EventRequest,
    },
    EvaluatorResponse {
        evaluation: Evaluation,
        json_patch: Value,
        signature: Signature,
    },
    ApproverResponse {
        approval: Approval,
    },
    ValidatorResponse {
        event_hash: DigestIdentifier,
        signature: Signature,
        governance_version: u64,
    },
    HigherGovernanceExpected {
        governance_id: DigestIdentifier,
        who_asked: KeyIdentifier,
    },
}

#[derive(Debug, Clone)]
pub enum EventResponse {
    Event(Result<DigestIdentifier, EventError>),
    NoResponse,
}
