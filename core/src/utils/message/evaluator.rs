pub use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{
    commons::models::event_preevaluation::EventPreEvaluation, evaluator::EvaluatorMessage,
};

pub fn create_evaluator_request(event_pre_eval: EventPreEvaluation) -> TapleMessages {
    TapleMessages::EvaluationMessage(EvaluatorMessage::AskForEvaluation(event_pre_eval))
}
