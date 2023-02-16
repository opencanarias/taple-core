pub mod db;
pub mod level_db;

use std::collections::{HashMap, HashSet};

use crate::{
    errors::SubjectError,
    identifier::DigestIdentifier,
    models::{
        event::Event,
        event_content::EventContent,
        event_request::EventRequest,
        signature::Signature,
        state::{LedgerState, Subject},
    },
};

pub trait TapleDB: Sized {
    fn get_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Option<Event>;

    fn get_events_by_range(
        &self,
        subject_id: &DigestIdentifier,
        from: Option<String>,
        quantity: isize,
    ) -> Vec<Event>;
    fn set_event(&self, subject_id: &DigestIdentifier, event: Event);

    fn get_signatures(&self, subject_id: &DigestIdentifier, sn: u64) -> Option<HashSet<Signature>>;

    fn set_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    );

    fn get_subject(&self, subject_id: &DigestIdentifier) -> Option<Subject>;

    fn set_subject(&self, subject_id: &DigestIdentifier, subject: Subject);

    fn set_negociating_true(&self, subject_id: &DigestIdentifier) -> Result<(), SubjectError>;

    fn apply_event_sourcing(&self, event_content: EventContent) -> Result<(), SubjectError>;

    fn get_all_heads(&self) -> HashMap<DigestIdentifier, LedgerState>;

    fn get_all_subjects(
        &self
    ) -> Vec<Subject>;

    fn get_subjects(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Vec<Subject>;

    fn get_governances(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Vec<Subject>;

    fn get_all_request(&self) -> Vec<EventRequest>;
    fn get_request(
        &self,
        subject_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Option<EventRequest>;
    fn set_request(&self, subject_id: &DigestIdentifier, request: EventRequest);
    fn del_request(
        &self,
        subject_id: &DigestIdentifier,
        request_id: &DigestIdentifier,
    ) -> Option<EventRequest>;

    fn get_controller_id(&self) -> Option<String>;
    fn set_controller_id(&self, controller_id: String);
}
