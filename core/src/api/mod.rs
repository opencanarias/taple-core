use std::collections::HashSet;
use crate::commons::models::approval::ApprovalStatus;
use crate::commons::models::request::TapleRequest;
use crate::signature::Signature;
use crate::{Acceptance, ApprovalPetitionData};
use crate::{KeyIdentifier, KeyDerivator};
use crate::commons::models::event::Event;
use crate::commons::models::event_request::EventRequest;
use crate::commons::models::state::SubjectData;
use crate::identifier::DigestIdentifier;

mod api;

pub use api::{NodeAPI, ApiModuleInterface};
pub(crate) use api::API;
pub use error::ApiError;

mod error;
mod inner_api;

#[derive(Debug, Clone)]
pub enum APICommands {
    GetSubjects(GetSubjects),
    GetGovernances(GetSubjects),
    GetSubject(GetSubject),
    GetEvent(DigestIdentifier, u64),
    GetEvents(GetEvents),
    #[cfg(feature = "aproval")]
    VoteResolve(Acceptance, DigestIdentifier),
    ExternalRequest(EventRequest),
    #[cfg(feature = "aproval")]
    GetPendingRequests,
    #[cfg(feature = "aproval")]
    GetSingleRequest(DigestIdentifier),
    SetPreauthorizedSubject(DigestIdentifier, HashSet<KeyIdentifier>),
    AddKeys(KeyDerivator),
    GetValidationProof(DigestIdentifier),
    GetRequest(DigestIdentifier),
    GetGovernanceSubjects(GetGovernanceSubjects),
    GetApproval(DigestIdentifier),
    GetApprovals(Option<String>),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum ApiResponses {
    GetSubjects(Result<Vec<SubjectData>, ApiError>),
    GetGovernances(Result<Vec<SubjectData>, ApiError>),
    GetSubject(Result<SubjectData, ApiError>),
    GetEvents(Result<Vec<Event>, ApiError>),
    HandleExternalRequest(Result<DigestIdentifier, ApiError>),
    #[cfg(feature = "aproval")]
    VoteResolve(Result<DigestIdentifier, ApiError>),
    #[cfg(feature = "aproval")]
    GetPendingRequests(Result<Vec<ApprovalPetitionData>, ApiError>),
    #[cfg(feature = "aproval")]
    GetSingleRequest(Result<ApprovalPetitionData, ApiError>),
    GetEvent(Result<Event, ApiError>),
    AddKeys(Result<KeyIdentifier, ApiError>),
    GetValidationProof(Result<HashSet<Signature>, ApiError>),
    GetRequest(Result<TapleRequest, ApiError>),
    GetGovernanceSubjects(Result<Vec<SubjectData>, ApiError>),
    GetApproval(Result<(ApprovalPetitionData, ApprovalStatus), ApiError>),
    GetApprovals(Result<Vec<ApprovalPetitionData>, ApiError>),
    ShutdownCompleted,
    SetPreauthorizedSubjectCompleted
}

#[derive(Debug, Clone)]
pub struct GetSubjects {
    pub namespace: String,
    pub from: Option<String>,
    pub quantity: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct GetSubject {
    pub subject_id: DigestIdentifier,
}

#[derive(Debug, Clone)]
pub struct GetEvents {
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