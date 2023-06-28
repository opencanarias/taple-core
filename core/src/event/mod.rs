use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    commons::models::{evaluation::EvaluationResponse, value_wrapper::ValueWrapper},
    identifier::DigestIdentifier,
    signature::{Signature, Signed},
    EventRequest, KeyIdentifier, ApprovalResponse,
};

use self::errors::EventError;

pub mod errors;
pub mod event_completer;
pub mod manager;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum EventCommand {
    Event {
        event_request: Signed<EventRequest>,
    },
    EvaluatorResponse {
        evaluator_response: Signed<EvaluationResponse>,
    },
    ApproverResponse {
        approval: Signed<ApprovalResponse>,
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
