use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    commons::models::{event_proposal::Evaluation, value_wrapper::ValueWrapper},
    identifier::DigestIdentifier,
    signature::{Signature, Signed},
    EventRequestType, KeyIdentifier, ApprovalContent,
};

use self::errors::EventError;

pub mod errors;
pub mod event_completer;
pub mod manager;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum EventCommand {
    Event {
        event_request: Signed<EventRequestType>,
    },
    EvaluatorResponse {
        evaluation: Evaluation,
        json_patch: ValueWrapper,
        signature: Signature,
    },
    ApproverResponse {
        approval: Signed<ApprovalContent>,
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
