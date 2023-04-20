use std::collections::HashSet;

use crate::{
    event_request::EventRequest, identifier::DigestIdentifier, signature::Signature, Event,
};

pub mod errors;
pub mod ledger;
pub mod manager;

#[derive(Debug, Clone)]
pub enum LedgerCommand {
    OwnEvent {
        event: Event,
        signatures: HashSet<Signature>,
    },
    OwnGenesis {
        event_request: EventRequest,
    },
    ExternalEvent {
        event: Event,
        signatures: HashSet<Signature>,
    },
    ExternalGenesis {

    },
    ExternalIntermediateEvent {
        event: Event,
    },
}

#[derive(Debug, Clone)]
pub enum LedgerResponse {
    NoResponse,
}
