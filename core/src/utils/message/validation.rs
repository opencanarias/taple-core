pub use crate::protocol::protocol_message_manager::TapleMessages;
use crate::validation::{ValidationCommand, ValidationEvent};

pub fn create_validator_request(validation_event: ValidationEvent) -> TapleMessages {
    TapleMessages::ValidationMessage(ValidationCommand::AskForValidation(validation_event))
}
