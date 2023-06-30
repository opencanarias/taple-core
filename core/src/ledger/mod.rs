use std::collections::HashSet;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::models::validation::ValidationProof, identifier::DigestIdentifier, signature::{Signature, Signed},
    KeyDerivator, KeyIdentifier, Event,
};

pub mod errors;
pub mod ledger;
pub mod manager;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum LedgerCommand {
    OwnEvent {
        event: Signed<Event>,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    },
    Genesis {
        event: Signed<Event>,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    },
    ExternalEvent {
        sender: KeyIdentifier,
        event: Signed<Event>,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    },
    ExternalIntermediateEvent {
        event: Signed<Event>,
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
    GetEvent(Result<Signed<Event>, errors::LedgerError>),
    GetNextGov(Result<(Signed<Event>, HashSet<Signature>), errors::LedgerError>),
    GetLCE(Result<(Signed<Event>, HashSet<Signature>), errors::LedgerError>),
    GenerateKey(Result<KeyIdentifier, errors::LedgerError>),
    NoResponse,
}
