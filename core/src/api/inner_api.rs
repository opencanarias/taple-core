use super::error::APIInternalError;
use super::ApiResponses;
use crate::approval::error::ApprovalErrorResponse;
use crate::approval::manager::{ApprovalAPI, ApprovalAPIInterface};
use crate::commons::models::Acceptance;
use crate::commons::self_signature_manager::{SelfSignatureInterface, SelfSignatureManager};
use crate::event::errors::EventError;
use crate::event::manager::{EventAPI, EventAPIInterface};
use crate::event::EventResponse;
use crate::identifier::Derivable;
// use crate::ledger::errors::LedgerManagerError;
use crate::{
    commons::{
        config::TapleSettings,
        crypto::KeyPair,
        identifier::DigestIdentifier,
        models::{
            event_request::{EventRequest, EventRequestType},
            state::SubjectData,
            timestamp::TimeStamp,
        },
    },
    DatabaseManager, DB,
};
use std::str::FromStr;

use super::{
    error::ApiError, GetAllSubjects, GetEventsOfSubject, GetSingleSubject as GetSingleSubjectAPI,
};

use crate::database::Error as DbError;

pub(crate) struct InnerAPI<D: DatabaseManager> {
    signature_manager: SelfSignatureManager,
    event_api: EventAPI,
    approval_api: ApprovalAPI,
    db: DB<D>,
}

const MAX_QUANTITY: isize = 100;

impl<D: DatabaseManager> InnerAPI<D> {
    pub fn new(
        keys: KeyPair,
        settings: &TapleSettings,
        event_api: EventAPI,
        db: DB<D>,
        approval_api: ApprovalAPI,
    ) -> Self {
        Self {
            signature_manager: SelfSignatureManager::new(keys, settings),
            event_api,
            approval_api,
            db,
        }
    }

    pub async fn handle_request(
        &self,
        request: EventRequestType,
    ) -> Result<ApiResponses, APIInternalError> {
        let timestamp = TimeStamp::now();
        let signature = self
            .signature_manager
            .sign(&(&request, &timestamp))
            .map_err(|_| APIInternalError::SignError)?;
        let request = EventRequest {
            request,
            timestamp,
            signature,
        };
        let EventResponse::Event(response) = self.event_api.send_event_request(request).await else {
            return Err(APIInternalError::UnexpectedManagerResponse);
        };
        match response {
            Ok(request_id) => return Ok(ApiResponses::HandleRequest(Ok(request_id))),
            Err(EventError::SubjectNotFound(subject_id)) => {
                return Ok(ApiResponses::HandleRequest(Err(ApiError::NotFound(
                    format!("Subject {} not found", subject_id),
                ))))
            }
            Err(EventError::SubjectNotOwned(str)) => {
                return Ok(ApiResponses::HandleRequest(Err(
                    ApiError::NotEnoughPermissions(format!("{}", str)),
                )))
            }
            Err(EventError::CreatingPermissionDenied) => {
                return Ok(ApiResponses::HandleRequest(Err(
                    ApiError::NotEnoughPermissions(format!("{}", response.unwrap_err())),
                )))
            }
            Err(error) => Ok(ApiResponses::HandleRequest(Err(error.into()))),
        }
    }

    pub async fn handle_external_request(
        &self,
        request: EventRequest,
    ) -> Result<ApiResponses, APIInternalError> {
        // Me llega una event request ya firmada. No debería ser de tipo Create. Hacemos esa comprobación y se la pasamos al manager
        // if let EventRequestType::Create(_) = request.request {
        //     return Ok(ApiResponses::HandleExternalRequest(Err(
        //         ApiError::InvalidParameters(String::from(
        //             " Event requests of type \"Create\" are not allowed",
        //         )),
        //     )));
        // }
        let EventResponse::Event(response) = self.event_api.send_event_request(request).await else {
            return Err(APIInternalError::UnexpectedManagerResponse);
        };
        log::error!("COMPLETA SEND EVENT REQUEST");
        Ok(ApiResponses::HandleExternalRequest(
            response.map_err(|e| ApiError::EventCreationError { source: e }),
        ))
    }

    pub async fn emit_vote(
        &self,
        request_id: DigestIdentifier,
        acceptance: Acceptance,
    ) -> Result<ApiResponses, APIInternalError> {
        // Es posible que en lugar de subject_id se prefiera un request_id
        let id_str = request_id.to_str();
        let result = self.approval_api.emit_vote(request_id, acceptance).await;
        match result {
            Ok(_) => return Ok(ApiResponses::VoteResolve(Ok(DigestIdentifier::default()))), // Cambiar al digestIdentifier del sujeto o de la misma request
            Err(ApprovalErrorResponse::ApprovalRequestNotFound) => {
                return Ok(ApiResponses::VoteResolve(Err(ApiError::NotFound(format!(
                    "Request {} not found",
                    id_str
                )))))
            }
            Err(ApprovalErrorResponse::APIChannelNotAvailable) => {
                return Err(APIInternalError::ChannelError)
            }
            _ => return Err(APIInternalError::UnexpectedManagerResponse),
        };
    }

    pub fn get_all_subjects(&self, data: GetAllSubjects) -> ApiResponses {
        let from = if data.from.is_none() {
            None
        } else {
            Some(format!("{}", data.from.unwrap()))
        };
        let quantity = if data.quantity.is_none() {
            MAX_QUANTITY
        } else {
            (data.quantity.unwrap() as isize).min(MAX_QUANTITY)
        };
        let result = match self.db.get_subjects(from, quantity) {
            Ok(subjects) => subjects,
            Err(error) => {
                return ApiResponses::GetAllSubjects(Err(ApiError::DatabaseError(
                    error.to_string(),
                )))
            }
        };
        let result = result
            .into_iter()
            .map(|subject| subject.into())
            .collect::<Vec<SubjectData>>();
        ApiResponses::GetAllSubjects(Ok(result))
    }

    pub async fn get_all_governances(&self, data: GetAllSubjects) -> ApiResponses {
        let from = if data.from.is_none() {
            None
        } else {
            Some(format!("{}", data.from.unwrap()))
        };
        let quantity = if data.quantity.is_none() {
            MAX_QUANTITY
        } else {
            (data.quantity.unwrap() as isize).min(MAX_QUANTITY)
        };
        let result = match self.db.get_governances(from, quantity) {
            Ok(subjects) => subjects,
            Err(error) => {
                return ApiResponses::GetAllGovernances(Err(ApiError::DatabaseError(
                    error.to_string(),
                )))
            }
        };
        let result = result
            .into_iter()
            .map(|subject| subject.into())
            .collect::<Vec<SubjectData>>();
        ApiResponses::GetAllGovernances(Ok(result))
    }

    pub async fn get_single_subject(&self, data: GetSingleSubjectAPI) -> ApiResponses {
        let id = &data.subject_id;
        let subject = match self.db.get_subject(id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => {
                return ApiResponses::GetSingleSubject(Err(ApiError::NotFound(format!(
                    "Subject {}",
                    data.subject_id.to_str()
                ))))
            }
            Err(error) => {
                return ApiResponses::GetSingleSubject(Err(ApiError::DatabaseError(
                    error.to_string(),
                )))
            }
        };
        ApiResponses::GetSingleSubject(Ok(subject.into()))
    }

    pub async fn get_events_of_subject(&self, data: GetEventsOfSubject) -> ApiResponses {
        let quantity = if data.quantity.is_none() {
            MAX_QUANTITY
        } else {
            (data.quantity.unwrap() as isize).min(MAX_QUANTITY)
        };
        let id = &data.subject_id;
        match self.db.get_events_by_range(id, data.from, quantity) {
            Ok(events) => ApiResponses::GetEventsOfSubject(Ok(events)),
            Err(error) => {
                ApiResponses::GetEventsOfSubject(Err(ApiError::DatabaseError(error.to_string())))
            }
        }
    }

    pub async fn get_pending_request(&self) -> ApiResponses {
        match self.approval_api.get_all_requests().await {
            Ok(data) => return ApiResponses::GetPendingRequests(Ok(data)),
            Err(error) => return ApiResponses::GetPendingRequests(Err(error.into())),
        }
    }

    pub async fn get_single_request(&self, request_id: DigestIdentifier) -> ApiResponses {
        match self
            .approval_api
            .get_single_request(request_id.clone())
            .await
        {
            Ok(data) => return ApiResponses::GetSingleRequest(Ok(data)),
            Err(ApprovalErrorResponse::ApprovalRequestNotFound) => {
                return ApiResponses::GetSingleRequest(Err(ApiError::NotFound(format!(
                    "Approval Request {} not found",
                    request_id.to_str()
                ))))
            }
            Err(error) => return ApiResponses::GetSingleRequest(Err(error.into())),
        }
    }
}

fn get_init_and_end<T>(
    from: Option<usize>,
    quantity: Option<usize>,
    data: &Vec<T>,
) -> (usize, usize) {
    let init = if from.is_some() { from.unwrap() } else { 0 };
    let end = if quantity.is_some() {
        let to = quantity.unwrap() + init;
        if to > data.len() {
            data.len()
        } else {
            to
        }
    } else {
        data.len()
    };
    (init, end)
}
