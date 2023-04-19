use crate::commons::models::Acceptance;
use crate::commons::models::event::Event;
use crate::commons::models::signature::Signature;
use crate::commons::models::state::SubjectData;
use crate::commons::{models::event_request::EventRequest};
use crate::event_request::{RequestPayload, RequestData};
use serde::{Serialize, Deserialize};
use utoipa::ToSchema;

mod api;
mod error;
mod inner_api;

pub use api::{ApiModuleInterface, NodeAPI, API};
pub use error::ApiError;

#[derive(Debug, Clone)]
pub enum APICommands {
    GetAllSubjects(GetAllSubjects),
    GetAllGovernances(GetAllSubjects),
    GetSingleSubject(GetSingleSubject),
    GetEventsOfSubject(GetEventsOfSubject),
    GetSignatures(GetSignatures),
    SimulateEvent(CreateEvent),
    VoteResolve(Acceptance, String),
    CreateRequest(CreateRequest),
    ExternalRequest(ExternalEventRequest),
    GetPendingRequests,
    GetSingleRequest(String),
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct GetAllSubjects {
    pub namespace: String,
    pub from: Option<String>,
    pub quantity: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GetSingleSubject {
    pub subject_id: String,
}

#[derive(Debug, Clone)]
pub struct CreateEvent {
    pub subject_id: String,
    pub payload: RequestPayload,
}

#[derive(Debug, Clone)]
pub struct CreateSubject {
    pub governance_id: String,
    pub schema_id: String,
    pub namespace: String,
    pub payload: RequestPayload,
}

/// Specification of the different types of available requests
#[derive(Debug, Clone)]
pub enum CreateRequest {
    Create(CreateType),
    State(StateType),
}

/// Request for a Create type event 
#[derive(Debug, Clone)]
pub struct CreateType {
    pub governance_id: String,
    pub schema_id: String,
    pub namespace: String,
    pub payload: RequestPayload,
}

/// Request for a State type event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateType {
    pub subject_id: String,
    pub payload: RequestPayload,
}

#[derive(Debug, Clone)]
pub struct CreateGovernance {
    pub payload: RequestPayload,
}

#[derive(Debug, Clone)]
pub struct GetEventsOfSubject {
    pub subject_id: String,
    pub from: Option<i64>,
    pub quantity: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct GetSignatures {
    pub subject_id: String,
    pub sn: u64,
    pub from: Option<usize>,
    pub quantity: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum APIResponses {
    GetAllSubjects(Result<Vec<SubjectData>, ApiError>),
    GetAllGovernances(Result<Vec<SubjectData>, ApiError>),
    GetSingleSubject(Result<SubjectData, ApiError>),
    GetEventsOfSubject(Result<Vec<Event>, ApiError>),
    GetSignatures(Result<Vec<Signature>, ApiError>),
    SimulateEvent(Result<SubjectData, ApiError>),
    VoteResolve(Result<(), ApiError>),
    GetPendingRequests(Result<Vec<EventRequest>, ApiError>),
    GetSingleRequest(Result<EventRequest, ApiError>),
    CreateRequest(Result<RequestData, ApiError>),
    ExternalRequest(Result<RequestData, ApiError>),
    ShutdownCompleted,
}

/// Data structure of a externa event request
#[derive(Debug, Clone, ToSchema, Serialize, Deserialize)]
pub struct ExternalEventRequest {
    pub request: StateType,
    pub timestamp: u64,
    pub signature: SignatureRequest,
}

/// Signature of a external event request
#[derive(Debug, Clone, PartialEq, ToSchema, Serialize, Deserialize)]
pub struct SignatureRequest {
    pub content: SignatureRequestContent,
    pub signature: String, // SignatureIdentifier,
}

/// Content of the signature of a external event request
#[derive(Debug, Clone, PartialEq, ToSchema, Serialize, Deserialize)]
pub struct SignatureRequestContent {
    pub signer: String,             // KeyIdentifier,
    pub event_content_hash: String, // DigestIdentifier,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Eq, Deserialize, ToSchema)]
#[allow(non_snake_case)]
pub struct StateRequestBodyUpper {
    pub State: StateRequestBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Eq, Deserialize, ToSchema)]
pub struct StateRequestBody {
    pub subject_id: String,
    pub payload: Payload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Eq, Deserialize, ToSchema)]
pub enum Payload {
    #[schema(value_type = Object)]
    Json(serde_json::Value),
    #[schema(value_type = Object)]
    JsonPatch(serde_json::Value),
}

impl Into<RequestPayload> for Payload {
    fn into(self) -> RequestPayload {
        match self {
            Self::Json(data) => RequestPayload::Json(serde_json::to_string(&data).unwrap()),
            Self::JsonPatch(data) => {
                RequestPayload::JsonPatch(serde_json::to_string(&data).unwrap())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExternalEventRequestBody {
    pub request: StateRequestBodyUpper,
    pub timestamp: u64,
    pub signature: SignatureRequest,
}

impl Into<ExternalEventRequest> for ExternalEventRequestBody {
    fn into(self) -> ExternalEventRequest {
        ExternalEventRequest {
            request: StateType {
                subject_id: self.request.State.subject_id,
                payload: self.request.State.payload.into(),
            },
            timestamp: self.timestamp,
            signature: self.signature,
        }
    }
}