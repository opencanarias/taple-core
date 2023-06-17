use crate::notary::{NotaryCommand, NotaryEvent};
pub use crate::protocol::protocol_message_manager::TapleMessages;

pub fn create_validator_request(notary_event: NotaryEvent) -> TapleMessages {
    TapleMessages::ValidationMessage(NotaryCommand::AskForNotary(notary_event))
}
