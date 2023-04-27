use std::collections::HashSet;

use crate::{
    event_request::EventRequest, identifier::DigestIdentifier, signature::Signature, Event, KeyIdentifier,
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
    Genesis {
        event_request: EventRequest,
    },
    ExternalEvent {
        sender: KeyIdentifier,
        event: Event,
        signatures: HashSet<Signature>,
    },
    ExternalIntermediateEvent {
        event: Event,
    },
    GetEvent {
        subject_id: DigestIdentifier,
        sn: u64,
    },
    GetLCE {
        subject_id: DigestIdentifier,
    },
}

#[derive(Debug, Clone)]
pub enum LedgerResponse {
    GetEvent(Result<Event, errors::LedgerError>),
    GetLCE(Result<(Event, HashSet<Signature>), errors::LedgerError>),
    NoResponse,
}
