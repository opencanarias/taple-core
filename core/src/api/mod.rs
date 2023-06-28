use crate::{signature::Signed};
use crate::commons::models::request::TapleRequest;
use crate::commons::models::state::SubjectData;
use crate::identifier::DigestIdentifier;
use crate::signature::Signature;
#[cfg(feature = "aproval")]
use crate::{KeyDerivator, KeyIdentifier, Event, EventRequest};
use std::collections::HashSet;

mod api;

pub(crate) use api::API;
pub use api::{ApiModuleInterface, NodeAPI};
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
    VoteResolve(bool, DigestIdentifier),
    ExternalRequest(Signed<EventRequest>),
    #[cfg(feature = "aproval")]
    GetPendingRequests,
    #[cfg(feature = "aproval")]
    GetSingleRequest(DigestIdentifier),
    SetPreauthorizedSubject(DigestIdentifier, HashSet<KeyIdentifier>),
    AddKeys(KeyDerivator),
    GetValidationProof(DigestIdentifier),
    GetRequest(DigestIdentifier),
    GetGovernanceSubjects(GetGovernanceSubjects),
    #[cfg(feature = "aproval")]
    GetApproval(DigestIdentifier),
    #[cfg(feature = "aproval")]
    GetApprovals(Option<String>),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum ApiResponses {
    GetSubjects(Result<Vec<SubjectData>, ApiError>),
    GetGovernances(Result<Vec<SubjectData>, ApiError>),
    GetSubject(Result<SubjectData, ApiError>),
    GetEvents(Result<Vec<Signed<Event>>, ApiError>),
    HandleExternalRequest(Result<DigestIdentifier, ApiError>),
    #[cfg(feature = "aproval")]
    VoteResolve(Result<DigestIdentifier, ApiError>),
    #[cfg(feature = "aproval")]
    GetPendingRequests(Result<Vec<ApprovalEntity>, ApiError>),
    #[cfg(feature = "aproval")]
    GetSingleRequest(Result<ApprovalEntity, ApiError>),
    GetEvent(Result<Signed<Event>, ApiError>),
    AddKeys(Result<KeyIdentifier, ApiError>),
    GetValidationProof(Result<HashSet<Signature>, ApiError>),
    GetRequest(Result<TapleRequest, ApiError>),
    GetGovernanceSubjects(Result<Vec<SubjectData>, ApiError>),
    #[cfg(feature = "aproval")]
    GetApproval(Result<ApprovalEntity, ApiError>),
    #[cfg(feature = "aproval")]
    GetApprovals(Result<Vec<ApprovalEntity>, ApiError>),
    ShutdownCompleted,
    SetPreauthorizedSubjectCompleted,
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
