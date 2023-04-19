use std::collections::HashSet;

use crate::{event_request::EventRequest, signature::Signature, Event};

pub mod errors;
pub mod ledger;
pub mod manager;

#[derive(Debug, Clone)]
pub enum LedgerCommand {
    EventValidated {
        event: Event,
        signatures: HashSet<Signature>,
    },
    Genesis {
        event_request: EventRequest,
    },
    EventPreValidated {
        event: Event,
    },
}

#[derive(Debug, Clone)]
pub enum LedgerResponse {
    NoResponse,
}
