use serde::{Deserialize, Serialize};

use crate::{
    commons::models::{
        approval::Approval,
        event_preevaluation::EventPreEvaluation,
        event_proposal::{Evaluation, EventProposal},
    },
    event_request::EventRequest,
    identifier::DigestIdentifier,
    message::TaskCommandContent,
    signature::Signature,
    Event, evaluator::compiler::NewGovVersion,
};

use self::errors::EventError;

pub mod errors;
pub mod event_completer;
pub mod manager;

#[derive(Debug, Clone)]
pub enum EventCommand {
    Event {
        event_request: EventRequest,
    },
    EvaluatorResponse {
        evaluation: Evaluation,
        json_patch: String,
        signature: Signature,
    },
    ApproverResponse {
        approval: Approval,
    },
    ValidatorResponse {
        signature: Signature,
    },
    NewGovVersion(NewGovVersion),
}

#[derive(Debug, Clone)]
pub enum EventResponse {
    Event(Result<DigestIdentifier, EventError>),
    NoResponse,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum EventMessages {
    EvaluationRequest(EventPreEvaluation),
    ApprovalRequest(EventProposal),
    ValidationRequest(Event),
}

impl TaskCommandContent for EventMessages {}
