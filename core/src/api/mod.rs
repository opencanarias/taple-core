use std::collections::HashSet;
#[cfg(feature = "aproval")]
use crate::{Acceptance, ApprovalPetitionData};
use crate::{KeyIdentifier};
use crate::commons::models::event::{Event, ValidationProof};
use crate::commons::models::event_request::EventRequest;
use crate::commons::models::state::SubjectData;
use crate::event_request::{EventRequestType};
use crate::identifier::DigestIdentifier;

mod api;

pub use api::{NodeAPI, ApiModuleInterface};
pub(crate) use api::API;
pub use error::ApiError;

mod error;
mod inner_api;

#[derive(Debug, Clone)]
pub enum APICommands {
    GetAllSubjects(GetAllSubjects),
    GetAllGovernances(GetAllSubjects),
    GetSingleSubject(GetSingleSubject),
    GetEventsOfSubject(GetEventsOfSubject),
    #[cfg(feature = "aproval")]
    VoteResolve(Acceptance, DigestIdentifier),
    HandleRequest(EventRequestType),
    ExternalRequest(EventRequest),
    #[cfg(feature = "aproval")]
    GetPendingRequests,
    #[cfg(feature = "aproval")]
    GetSingleRequest(DigestIdentifier),
    SetPreauthorizedSubject(DigestIdentifier, HashSet<KeyIdentifier>),
    ExpectingTransfer(DigestIdentifier),
    GetValidationProof(DigestIdentifier),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum ApiResponses {
    GetAllSubjects(Result<Vec<SubjectData>, ApiError>),
    GetAllGovernances(Result<Vec<SubjectData>, ApiError>),
    GetSingleSubject(Result<SubjectData, ApiError>),
    GetEventsOfSubject(Result<Vec<Event>, ApiError>),
    HandleExternalRequest(Result<DigestIdentifier, ApiError>),
    #[cfg(feature = "aproval")]
    VoteResolve(Result<DigestIdentifier, ApiError>),
    HandleRequest(Result<DigestIdentifier, ApiError>), // Borrar RequestData
    #[cfg(feature = "aproval")]
    GetPendingRequests(Result<Vec<ApprovalPetitionData>, ApiError>),
    #[cfg(feature = "aproval")]
    GetSingleRequest(Result<ApprovalPetitionData, ApiError>),
    ExpectingTransfer(Result<KeyIdentifier, ApiError>),
    GetValidationProof(Result<ValidationProof, ApiError>),
    ShutdownCompleted,
    SetPreauthorizedSubjectCompleted
}

#[derive(Debug, Clone)]
pub struct GetAllSubjects {
    pub namespace: String,
    pub from: Option<String>,
    pub quantity: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GetSingleSubject {
    pub subject_id: DigestIdentifier,
}

#[derive(Debug, Clone)]
pub struct GetEventsOfSubject {
    pub subject_id: DigestIdentifier,
    pub from: Option<i64>,
    pub quantity: Option<i64>,
}
