use std::collections::HashSet;

use crate::{
    event_request::EventRequest, identifier::DigestIdentifier, signature::Signature, Event,
};

pub mod errors;
pub mod ledger;
pub mod manager;

#[derive(Debug, Clone)]
pub enum LedgerCommand {
    EventValidated {
        subject_id: DigestIdentifier,
        event: Event,
        signatures: HashSet<Signature>,
    },
    Genesis {
        event_request: EventRequest,
    },
    EventPreValidated {
        subject_id: DigestIdentifier,
        event: Event,
    },
}

#[derive(Debug, Clone)]
pub enum LedgerResponse {
    NoResponse,
}
