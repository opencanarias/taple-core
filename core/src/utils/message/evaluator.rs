pub use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{commons::models::evaluation::EvaluationRequest, evaluator::EvaluatorMessage};

pub fn create_evaluator_request(event_pre_eval: EvaluationRequest) -> TapleMessages {
    TapleMessages::EvaluationMessage(EvaluatorMessage::AskForEvaluation(event_pre_eval))
}
