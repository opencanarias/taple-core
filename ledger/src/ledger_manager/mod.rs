use std::collections::{HashMap, HashSet};

use commons::{
    identifier::{DigestIdentifier, KeyIdentifier},
    models::{
        event::Event,
        event_request::EventRequest,
        signature::Signature,
        state::{LedgerState, Subject, SubjectData},
    },
};

mod ledger;
mod manager;
pub use manager::LedgerAPI;
pub use manager::LedgerInterface;
pub use manager::LedgerManager;

#[derive(Debug, Clone)]
pub enum EventSN {
    SN(u64),
    HEAD,
}

#[derive(Debug)]
pub enum CommandManagerMessage {
    GetEvent {
        subject_id: DigestIdentifier,
        sn: EventSN,
    },
    GetSignatures {
        subject_id: DigestIdentifier,
        sn: EventSN,
    },
    GetSigners {
        subject_id: DigestIdentifier,
        sn: EventSN,
    },
    GetSubject {
        subject_id: DigestIdentifier,
    },
    GetSubjects {
        namespace: String,
    },
    GetSubjectsRaw {
        namespace: String,
    },
    PutEvent(Event),
    PutSignatures {
        signatures: HashSet<Signature>,
        sn: u64,
        subject_id: DigestIdentifier,
    },
    Init,
    CreateEvent(EventRequest, bool),
}

#[derive(Debug, PartialEq)]
pub enum CommandManagerResponse {
    GetEventResponse {
        event: Event,
        ledger_state: LedgerState,
    },
    GetSignaturesResponse {
        signatures: HashSet<Signature>,
        ledger_state: LedgerState,
    },
    GetSignersResponse {
        signers: HashSet<KeyIdentifier>,
        ledger_state: LedgerState,
    },
    GetSubjectResponse {
        subject: SubjectData,
    },
    GetSubjectsResponse {
        subjects: Vec<SubjectData>,
    },
    GetSubjectsRawResponse {
        subjects: Vec<Subject>,
    },
    PutEventResponse {
        ledger_state: LedgerState,
    },
    PutSignaturesResponse {
        sn: u64,
        signers: HashSet<KeyIdentifier>,
        signers_left: HashSet<KeyIdentifier>,
        ledger_state: LedgerState,
    },
    InitResponse(HashMap<DigestIdentifier, LedgerState>),
    CreateEventResponse(Event, LedgerState),
}
