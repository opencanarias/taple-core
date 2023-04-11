use serde::{Serialize, Deserialize};

use crate::{identifier::DigestIdentifier, message::TaskCommandContent};

use self::errors::EventError;

pub mod manager;
pub mod event_completer;
pub mod errors;

#[derive(Debug, Clone)]
pub enum EventCommand {
    Event{

    },
    EvaluatorResponse {

    },
    ApproverResponse {

    },
    NotaryResponse {

    },
}

#[derive(Debug, Clone)]
pub enum EventResponse {
    Event(Result<DigestIdentifier, EventError>),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum EventMessages {

}

impl TaskCommandContent for EventMessages {}