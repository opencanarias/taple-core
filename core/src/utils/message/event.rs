use serde_json::Value;

pub use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{signature::Signed, ApprovalResponse, EvaluationResponse};

pub fn create_evaluator_response(evaluator_response: Signed<EvaluationResponse>) -> TapleMessages {
    TapleMessages::EventMessage(crate::event::EventCommand::EvaluatorResponse {
        evaluator_response,
    })
}

pub fn create_approver_response(approval: Signed<ApprovalResponse>) -> TapleMessages {
    TapleMessages::EventMessage(crate::event::EventCommand::ApproverResponse { approval: approval })
}
