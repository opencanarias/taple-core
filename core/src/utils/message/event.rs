use serde_json::Value;

pub use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{
    commons::models::{approval::Approval, event_proposal::Evaluation, Acceptance, value_wrapper::ValueWrapper},
    identifier::DigestIdentifier,
    signature::Signature,
};

pub fn create_evaluator_response(
    preevaluation_hash: DigestIdentifier,
    state_hash: DigestIdentifier,
    governance_version: u64,
    acceptance: Acceptance,
    approval_required: bool,
    json_patch: ValueWrapper,
    signature: Signature,
) -> TapleMessages {
    TapleMessages::EventMessage(crate::event::EventCommand::EvaluatorResponse {
        evaluation: Evaluation {
            preevaluation_hash,
            state_hash,
            governance_version,
            acceptance,
            approval_required,
        },
        json_patch,
        signature,
    })
}

pub fn create_approver_response(approval: Approval) -> TapleMessages {
    TapleMessages::EventMessage(crate::event::EventCommand::ApproverResponse { approval: approval })
}
