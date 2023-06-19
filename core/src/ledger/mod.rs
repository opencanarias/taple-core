use std::collections::HashSet;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::models::event::ValidationProof, event_request::EventRequest,
    identifier::DigestIdentifier, signature::Signature, Event, KeyIdentifier, KeyDerivator,
};

pub mod errors;
pub mod ledger;
pub mod manager;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum LedgerCommand {
    OwnEvent {
        event: Event,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    },
    Genesis {
        event: Event,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    },
    ExternalEvent {
        sender: KeyIdentifier,
        event: Event,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    },
    ExternalIntermediateEvent {
        event: Event,
    },
    GetEvent {
        who_asked: KeyIdentifier,
        subject_id: DigestIdentifier,
        sn: u64,
    },
    GetNextGov {
        who_asked: KeyIdentifier,
        subject_id: DigestIdentifier,
        sn: u64,
    },
    GetLCE {
        who_asked: KeyIdentifier,
        subject_id: DigestIdentifier,
    },
    GenerateKey(KeyDerivator),
}

#[derive(Debug, Clone)]
pub enum LedgerResponse {
    GetEvent(Result<Event, errors::LedgerError>),
    GetNextGov(Result<(Event, HashSet<Signature>), errors::LedgerError>),
    GetLCE(Result<(Event, HashSet<Signature>), errors::LedgerError>),
    GenerateKey(Result<KeyIdentifier, errors::LedgerError>),
    NoResponse,
}
