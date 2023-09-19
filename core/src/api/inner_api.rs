use super::error::APIInternalError;
use super::{ApiResponses, GetAllowedSubjects};
#[cfg(feature = "approval")]
use crate::approval::error::ApprovalErrorResponse;
#[cfg(feature = "approval")]
use crate::approval::manager::{ApprovalAPI, ApprovalAPIInterface};
use crate::authorized_subjecs::manager::AuthorizedSubjectsAPI;
use crate::event::errors::EventError;
use crate::event::manager::{EventAPI, EventAPIInterface};
use crate::event::EventResponse;
use crate::identifier::Derivable;
use crate::ledger::manager::{EventManagerAPI, EventManagerInterface};
use crate::signature::Signed;
#[cfg(feature = "approval")]
use crate::ApprovalState;
use crate::{KeyDerivator, KeyIdentifier};
// use crate::ledger::errors::LedgerManagerError;
use crate::{
    commons::{
        identifier::DigestIdentifier,
        models::{request::EventRequest, state::SubjectData},
    },
    DatabaseCollection, DB,
};
use std::collections::HashSet;

use super::{
    error::ApiError, GetEvents, GetGovernanceSubjects, GetSubject as GetSingleSubjectAPI,
    GetSubjects,
};

use crate::database::DatabaseError as DbError;

pub(crate) struct InnerApi<C: DatabaseCollection> {
    event_api: EventAPI,
    #[cfg(feature = "approval")]
    approval_api: ApprovalAPI,
    authorized_subjects_api: AuthorizedSubjectsAPI,
    ledger_api: EventManagerAPI,
    db: DB<C>,
}

const MAX_QUANTITY: isize = 100;

impl<C: DatabaseCollection> InnerApi<C> {
    pub fn new(
        event_api: EventAPI,
        authorized_subjects_api: AuthorizedSubjectsAPI,
        db: DB<C>,
        #[cfg(feature = "approval")] approval_api: ApprovalAPI,
        ledger_api: EventManagerAPI,
    ) -> Self {
        Self {
            event_api,
            #[cfg(feature = "approval")]
            approval_api,
            authorized_subjects_api,
            db,
            ledger_api,
        }
    }

    pub async fn handle_external_request(
        &self,
        request: Signed<EventRequest>,
    ) -> Result<ApiResponses, APIInternalError> {
        let EventResponse::Event(response) = self.event_api.send_event_request(request).await else {
            return Err(APIInternalError::UnexpectedManagerResponse);
        };
        match response {
            Ok(data) => Ok(ApiResponses::HandleExternalRequest(Ok(data))),
            Err(EventError::PublicKeyIsEmpty) => Ok(ApiResponses::HandleExternalRequest(Err(
                ApiError::InvalidParameters(format!("{}", response.unwrap_err())),
            ))),
            Err(error) => Ok(ApiResponses::HandleExternalRequest(Err(
                ApiError::EventCreationError { source: error },
            ))),
        }
    }

    #[cfg(feature = "approval")]
    pub async fn emit_vote(
        &self,
        request_id: DigestIdentifier,
        acceptance: bool,
    ) -> Result<ApiResponses, APIInternalError> {
        // Es posible que en lugar de subject_id se prefiera un request_id
        let id_str = request_id.to_str();
        let result = self.approval_api.emit_vote(request_id, acceptance).await;
        match result {
            Ok(data) => return Ok(ApiResponses::VoteResolve(Ok(data))), // Cambiar al digestIdentifier del sujeto o de la misma request
            Err(ApprovalErrorResponse::RequestNotFound) => {
                return Ok(ApiResponses::VoteResolve(Err(ApiError::NotFound(format!(
                    "Request {} not found",
                    id_str
                )))))
            }
            Err(ApprovalErrorResponse::RequestAlreadyResponded) => {
                return Ok(ApiResponses::VoteResolve(Err(ApiError::Conflict(format!(
                    "Request already responded"
                )))))
            }
            Err(ApprovalErrorResponse::APIChannelNotAvailable) => {
                return Err(APIInternalError::ChannelError)
            }
            _ => return Err(APIInternalError::UnexpectedManagerResponse),
        };
    }

    fn get_from_and_quantity(&self, data: GetSubjects) -> (Option<String>, isize) {
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
        (from, quantity)
    }

    pub fn get_subjects_by_governance(
        &self,
        data: GetSubjects,
        governance_id: DigestIdentifier,
    ) -> ApiResponses {
        let (from, quantity) = self.get_from_and_quantity(data);
        let result = match self
            .db
            .get_governance_subjects(&governance_id, from, quantity)
        {
            Ok(subjects) => subjects,
            Err(error) => {
                return ApiResponses::GetSubjects(Err(ApiError::DatabaseError(error.to_string())))
            }
        };
        let result = result
            .into_iter()
            .map(|subject| subject.into())
            .collect::<Vec<SubjectData>>();
        ApiResponses::GetSubjectByGovernance(Ok(result))
    }

    pub fn get_all_subjects(&self, data: GetSubjects) -> ApiResponses {
        let (from, quantity) = self.get_from_and_quantity(data);
        let result = match self.db.get_subjects(from, quantity) {
            Ok(subjects) => subjects,
            Err(error) => {
                return ApiResponses::GetSubjects(Err(ApiError::DatabaseError(error.to_string())))
            }
        };
        let result = result
            .into_iter()
            .map(|subject| subject.into())
            .collect::<Vec<SubjectData>>();
        ApiResponses::GetSubjects(Ok(result))
    }

    pub async fn get_all_governances(&self, data: GetSubjects) -> ApiResponses {
        let (from, quantity) = self.get_from_and_quantity(data);
        let result = match self.db.get_governances(from, quantity) {
            Ok(subjects) => subjects,
            Err(error) => {
                return ApiResponses::GetGovernances(Err(ApiError::DatabaseError(
                    error.to_string(),
                )))
            }
        };
        let result = result
            .into_iter()
            .map(|subject| subject.into())
            .collect::<Vec<SubjectData>>();
        ApiResponses::GetGovernances(Ok(result))
    }

    pub async fn get_single_subject(&self, data: GetSingleSubjectAPI) -> ApiResponses {
        let id = &data.subject_id;
        let subject = match self.db.get_subject(id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => {
                return ApiResponses::GetSubject(Err(ApiError::NotFound(format!(
                    "Subject {}",
                    data.subject_id.to_str()
                ))))
            }
            Err(error) => {
                return ApiResponses::GetSubject(Err(ApiError::DatabaseError(error.to_string())))
            }
        };
        ApiResponses::GetSubject(Ok(subject.into()))
    }

    pub async fn get_request(&self, request_id: DigestIdentifier) -> ApiResponses {
        log::debug!("SE LLAMA A GET REQUEST antes de db");
        match self.db.get_taple_request(&request_id) {
            Ok(request) => {
                log::debug!("FUNCIONÓ EL GET DE LA DB");
                ApiResponses::GetRequest(Ok(request))
            }
            Err(DbError::EntryNotFound) => {
                log::debug!("entry not found apra get request");
                return ApiResponses::GetRequest(Err(ApiError::NotFound(format!(
                    "Request {}",
                    request_id.to_str()
                ))));
            }
            Err(error) => {
                log::debug!("ENTRY ERROR DE DATABASE");
                return ApiResponses::GetRequest(Err(ApiError::DatabaseError(error.to_string())));
            }
        }
    }

    pub fn get_event(&self, subject_id: DigestIdentifier, sn: u64) -> ApiResponses {
        match self.db.get_event(&subject_id, sn) {
            Ok(event) => ApiResponses::GetEvent(Ok(event)),
            Err(DbError::EntryNotFound) => ApiResponses::GetEvent(Err(ApiError::NotFound(
                format!("Event {} of subejct {}", sn, subject_id.to_str()),
            ))),
            Err(error) => ApiResponses::GetEvent(Err(ApiError::DatabaseError(error.to_string()))),
        }
    }

    pub async fn get_events_of_subject(&self, data: GetEvents) -> ApiResponses {
        let quantity = if data.quantity.is_none() {
            MAX_QUANTITY
        } else {
            (data.quantity.unwrap() as isize).min(MAX_QUANTITY)
        };
        let id = &data.subject_id;
        match self.db.get_events_by_range(id, data.from, quantity) {
            Ok(events) => ApiResponses::GetEvents(Ok(events)),
            Err(error) => ApiResponses::GetEvents(Err(ApiError::DatabaseError(error.to_string()))),
        }
    }

    #[cfg(feature = "approval")]
    pub async fn get_pending_request(&self) -> ApiResponses {
        match self.approval_api.get_all_requests().await {
            Ok(data) => return ApiResponses::GetPendingRequests(Ok(data)),
            Err(error) => return ApiResponses::GetPendingRequests(Err(error.into())),
        }
    }

    #[cfg(feature = "approval")]
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

    pub async fn set_preauthorized_subject(
        &self,
        subject_id: DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    ) -> Result<ApiResponses, APIInternalError> {
        if let Err(error) = self
            .authorized_subjects_api
            .new_authorized_subject(subject_id, providers)
            .await
        {
            return Err(APIInternalError::DatabaseError(error.to_string()));
        }
        Ok(ApiResponses::SetPreauthorizedSubjectCompleted)
    }

    pub async fn get_all_preauthorized_subjects_and_providers(
        &self,
        data: GetAllowedSubjects,
    ) -> Result<ApiResponses, APIInternalError> {
        let quantity = if data.quantity.is_none() {
            MAX_QUANTITY
        } else {
            (data.quantity.unwrap() as isize).min(MAX_QUANTITY)
        };
        match self
            .db
            .get_allowed_subjects_and_providers(data.from, quantity)
        {
            Ok(data) => Ok(ApiResponses::GetAllPreauthorizedSubjects(Ok(data))),
            Err(error) => Err(APIInternalError::DatabaseError(error.to_string())),
        }
    }

    pub async fn generate_keys(
        &self,
        derivator: KeyDerivator,
    ) -> Result<ApiResponses, APIInternalError> {
        match self.ledger_api.generate_keys(derivator).await {
            Ok(public_key) => Ok(ApiResponses::AddKeys(Ok(public_key))),
            Err(error) => Err(APIInternalError::DatabaseError(error.to_string())),
        }
    }

    pub async fn get_validation_proof(&self, subject_id: DigestIdentifier) -> ApiResponses {
        let result = match self.db.get_validation_proof(&subject_id) {
            Ok(vproof) => vproof,
            Err(error) => {
                return ApiResponses::GetValidationProof(Err(ApiError::DatabaseError(
                    error.to_string(),
                )))
            }
        };
        ApiResponses::GetValidationProof(Ok(result))
    }

    pub async fn get_governance_subjects(&self, data: GetGovernanceSubjects) -> ApiResponses {
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
        let result = match self
            .db
            .get_governance_subjects(&data.governance_id, from, quantity)
        {
            Ok(subjects) => subjects,
            Err(error) => {
                return ApiResponses::GetGovernanceSubjects(Err(ApiError::DatabaseError(
                    error.to_string(),
                )))
            }
        };
        let result = result
            .into_iter()
            .map(|subject| subject.into())
            .collect::<Vec<SubjectData>>();
        ApiResponses::GetGovernanceSubjects(Ok(result))
    }

    #[cfg(feature = "approval")]
    pub async fn get_approval(&self, subject_id: DigestIdentifier) -> ApiResponses {
        let result = match self.db.get_approval(&subject_id) {
            Ok(approval) => approval,
            Err(error) => {
                return ApiResponses::GetApproval(Err(ApiError::DatabaseError(error.to_string())))
            }
        };
        ApiResponses::GetApproval(Ok(result))
    }

    #[cfg(feature = "approval")]
    pub async fn get_approvals(
        &self,
        status: Option<ApprovalState>,
        from: Option<String>,
        quantity: Option<i64>,
    ) -> ApiResponses {
        let quantity = if quantity.is_none() {
            MAX_QUANTITY
        } else {
            let tmp = quantity.unwrap();
            if tmp == 0 {
                MAX_QUANTITY
            } else {
                tmp as isize
            }
        };
        let result = match self.db.get_approvals(status, from, quantity) {
            Ok(approvals) => approvals,
            Err(error) => {
                return ApiResponses::GetApprovals(Err(ApiError::DatabaseError(error.to_string())))
            }
        };
        ApiResponses::GetApprovals(Ok(result))
    }
}

#[allow(dead_code)]
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
