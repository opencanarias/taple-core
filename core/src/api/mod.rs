#[cfg(feature = "approval")]
use crate::commons::models::approval::ApprovalEntity;
use crate::commons::models::request::TapleRequest;
use crate::commons::models::state::SubjectData;
use crate::identifier::DigestIdentifier;
use crate::signature::Signature;
use crate::signature::Signed;
use crate::{ApprovalState, Event, EventRequest, KeyDerivator, KeyIdentifier, ValidationProof};
use std::collections::HashSet;

mod api;

pub use api::Api;
pub(crate) use api::ApiManager;
pub use error::ApiError;

mod error;
mod inner_api;

#[derive(Debug, Clone)]
pub enum APICommands {
    GetSubjects(GetSubjects),
    GetSubjectByGovernance(GetSubjects, DigestIdentifier),
    GetGovernances(GetSubjects),
    GetSubject(GetSubject),
    GetEvent(DigestIdentifier, u64),
    GetEvents(GetEvents),
    #[cfg(feature = "approval")]
    VoteResolve(bool, DigestIdentifier),
    ExternalRequest(Signed<EventRequest>),
    #[cfg(feature = "approval")]
    GetPendingRequests,
    #[cfg(feature = "approval")]
    GetSingleRequest(DigestIdentifier),
    SetPreauthorizedSubject(DigestIdentifier, HashSet<KeyIdentifier>),
    GetAllPreauthorizedSubjects(GetAllowedSubjects),
    AddKeys(KeyDerivator),
    GetValidationProof(DigestIdentifier),
    GetRequest(DigestIdentifier),
    GetGovernanceSubjects(GetGovernanceSubjects),
    #[cfg(feature = "approval")]
    GetApproval(DigestIdentifier),
    #[cfg(feature = "approval")]
    GetApprovals(GetApprovals),
}

#[derive(Debug, Clone)]
pub enum ApiResponses {
    GetSubjects(Result<Vec<SubjectData>, ApiError>),
    GetSubjectByGovernance(Result<Vec<SubjectData>, ApiError>),
    GetGovernances(Result<Vec<SubjectData>, ApiError>),
    GetSubject(Result<SubjectData, ApiError>),
    GetEvents(Result<Vec<Signed<Event>>, ApiError>),
    HandleExternalRequest(Result<DigestIdentifier, ApiError>),
    #[cfg(feature = "approval")]
    VoteResolve(Result<ApprovalEntity, ApiError>),
    #[cfg(feature = "approval")]
    GetPendingRequests(Result<Vec<ApprovalEntity>, ApiError>),
    #[cfg(feature = "approval")]
    GetSingleRequest(Result<ApprovalEntity, ApiError>),
    GetEvent(Result<Signed<Event>, ApiError>),
    AddKeys(Result<KeyIdentifier, ApiError>),
    GetValidationProof(Result<(HashSet<Signature>, ValidationProof), ApiError>),
    GetRequest(Result<TapleRequest, ApiError>),
    GetGovernanceSubjects(Result<Vec<SubjectData>, ApiError>),
    #[cfg(feature = "approval")]
    GetApproval(Result<ApprovalEntity, ApiError>),
    #[cfg(feature = "approval")]
    GetApprovals(Result<Vec<ApprovalEntity>, ApiError>),
    SetPreauthorizedSubjectCompleted,
    GetAllPreauthorizedSubjects(Result<Vec<(DigestIdentifier, HashSet<KeyIdentifier>)>, ApiError>),
}

#[derive(Debug, Clone)]
pub struct GetApprovals {
    pub state: Option<ApprovalState>,
    pub from: Option<String>,
    pub quantity: Option<i64>,
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

#[derive(Debug, Clone)]
pub struct GetAllowedSubjects {
    pub from: Option<String>,
    pub quantity: Option<i64>,
}
