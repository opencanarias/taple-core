use std::collections::HashSet;
use crate::commons::models::request::TapleRequest;
use crate::commons::models::approval::ApprovalStatus;
use crate::signature::Signature;
#[cfg(feature = "aproval")]
use crate::Acceptance;
use crate::KeyIdentifier;
use crate::ApprovalPetitionData;
use crate::commons::models::event::Event;
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
    GetEvent(DigestIdentifier, u64),
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
    GenerateKeys,
    GetValidationProof(DigestIdentifier),
    GetRequest(DigestIdentifier),
    GetGovernanceSubjects(GetGovernanceSubjects),
    GetApproval(DigestIdentifier),
    GetApprovals(Option<String>),
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
    GetEvent(Result<Event, ApiError>),
    GenerateKeys(Result<KeyIdentifier, ApiError>),
    GetValidationProof(Result<HashSet<Signature>, ApiError>),
    GetRequest(Result<TapleRequest, ApiError>),
    GetGovernanceSubjects(Result<Vec<SubjectData>, ApiError>),
    GetApproval(Result<(ApprovalPetitionData, ApprovalStatus), ApiError>),
    GetApprovals(Result<Vec<ApprovalPetitionData>, ApiError>),
    ShutdownCompleted,
    SetPreauthorizedSubjectCompleted
}

#[derive(Debug, Clone)]
pub struct GetAllSubjects {
    pub namespace: String,
    pub from: Option<String>,
    pub quantity: Option<i64>,
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

#[derive(Debug, Clone)]
pub struct GetGovernanceSubjects {
    pub governance_id: DigestIdentifier,
    pub from: Option<String>,
    pub quantity: Option<i64>,
}