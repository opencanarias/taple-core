pub use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{
  notary::{NotaryCommand, NotaryEvent}, Event,
};

pub fn create_validator_request(notary_event: Event) -> TapleMessages {
    TapleMessages::ValidationMessage(NotaryCommand::NotaryEvent(notary_event))
}
