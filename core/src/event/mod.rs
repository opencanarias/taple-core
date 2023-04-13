use serde::{Serialize, Deserialize};

use crate::{identifier::DigestIdentifier, message::TaskCommandContent, commons::models::{event_proposal::{Evaluation, EventProposal}, approval::Approval, event_preevaluation::EventPreEvaluation}, signature::Signature, event_request::EventRequest};

use self::errors::EventError;

pub mod manager;
pub mod event_completer;
pub mod errors;

#[derive(Debug, Clone)]
pub enum EventCommand {
    Event{
        event_request: EventRequest,
    },
    EvaluatorResponse {
        evaluation: Evaluation,
        signature: Signature,
    },
    ApproverResponse {
        approval: Approval,
    },
}

#[derive(Debug, Clone)]
pub enum EventResponse {
    Event(Result<DigestIdentifier, EventError>),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum EventMessages {
    EvaluationRequest(EventPreEvaluation),
    ApprovalRequest(EventProposal),
}

impl TaskCommandContent for EventMessages {}