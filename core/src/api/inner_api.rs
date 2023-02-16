use super::{CreateRequest as ApiCreateRequest, ExternalEventRequest};
use chrono::Utc;
use commons::{
    bd::{db::DB, TapleDB},
    config::TapleSettings,
    crypto::KeyPair,
    identifier::{derive::KeyDerivator, DigestIdentifier, KeyIdentifier, SignatureIdentifier},
    models::{
        approval_signature::Acceptance,
        event_content::{EventContent, Metadata},
        event_request::{CreateRequest, EventRequest, EventRequestType, StateRequest},
        signature::{Signature, SignatureContent},
        state::SubjectData,
    },
};
use governance::error::RequestError;
use ledger::errors::LedgerManagerError;
use protocol::{
    command_head_manager::{
        manager::CommandAPI,
        manager::CommandManagerInterface,
        self_signature_manager::{SelfSignatureInterface, SelfSignatureManager},
    },
    errors::{ResponseError, EventCreationError},
    request_manager::manager::{RequestManagerAPI, RequestManagerInterface},
};
use std::{collections::HashSet, str::FromStr};

use super::{
    error::ApiError, APIResponses, CreateEvent, GetAllSubjects, GetEventsOfSubject, GetSignatures,
    GetSingleSubject as GetSingleSubjectAPI,
};

pub(crate) struct InnerAPI {
    signature_manager: SelfSignatureManager,
    command_api: CommandAPI,
    request_api: RequestManagerAPI,
    db: DB,
}

const MAX_QUANTITY: isize = 100;

impl InnerAPI {
    pub fn new(
        keys: KeyPair,
        settings: &TapleSettings,
        command_api: CommandAPI,
        db: DB,
        request_api: RequestManagerAPI,
    ) -> Self {
        Self {
            signature_manager: SelfSignatureManager::new(keys, settings),
            command_api,
            request_api,
            db,
        }
    }

    pub async fn create_request(&self, data: ApiCreateRequest) -> APIResponses {
        let request = match data {
            ApiCreateRequest::Create(data) => {
                let Ok(id) = DigestIdentifier::from_str(&data.governance_id) else {
                    return APIResponses::CreateRequest(Err(ApiError::InvalidParameters(format!("GovernanceID {}", data.governance_id))));
                };
                EventRequestType::Create(CreateRequest {
                    governance_id: id,
                    schema_id: data.schema_id.clone(),
                    namespace: data.namespace.clone(),
                    payload: data.payload.into(),
                })
            }
            ApiCreateRequest::State(data) => {
                let Ok(id) = DigestIdentifier::from_str(&data.subject_id) else {
                    return APIResponses::CreateRequest(Err(ApiError::InvalidParameters(format!("SubjectID {}", data.subject_id))));
                };
                EventRequestType::State(StateRequest {
                    subject_id: id,
                    payload: data.payload.into(),
                })
            }
        };
        let timestamp = Utc::now().timestamp_millis();
        let Ok(signature) = self.signature_manager.sign(&(&request, timestamp)) else {
            return APIResponses::CreateRequest(Err(ApiError::SignError));
        };
        let event_request = EventRequest {
            request,
            timestamp,
            signature,
            approvals: HashSet::new(),
        };
        let result = self.request_api.event_request(event_request).await;
        match result {
            Ok(result) => APIResponses::CreateRequest(Ok(result)),
            Err(ResponseError::EventCreationError { source }) => {
                match &source {
                    EventCreationError::EventCreationFailed { source: ledger_error } => {
                        match &ledger_error {
                            LedgerManagerError::GovernanceError(RequestError::GovernanceNotFound(governance_id)) => {
                                APIResponses::CreateRequest(Err(
                                    ApiError::NotFound(format!("Governance {}", governance_id))
                                ))
                            },
                            LedgerManagerError::GovernanceError(RequestError::SchemaNotFound(schema_id)) => {
                                APIResponses::CreateRequest(Err(
                                    ApiError::NotFound(format!("Schema {}", schema_id))
                                ))
                            },
                            _ => APIResponses::CreateRequest(Err(source.into()))
                        }
                    },
                    _ => APIResponses::CreateRequest(Err(source.into()))
                }
            }
            Err(ResponseError::SubjectNotFound) => {
                APIResponses::CreateRequest(Err(ApiError::NotFound(format!("Subject"))))
            }
            Err(ResponseError::SchemaNotFound(schema_id)) => APIResponses::CreateRequest(Err(
                ApiError::NotFound(format!("Schema {}", schema_id)),
            )),
            Err(ResponseError::NotOwnerOfSubject) => APIResponses::CreateRequest(Err(
                ApiError::NotEnoughPermissions(format!("{}", result.unwrap_err())),
            )),
            Err(error) => APIResponses::CreateRequest(Err(error.into())),
        }
    }

    pub async fn external_request(&self, event_request: ExternalEventRequest) -> APIResponses {
        // Hacer transformacion de datos de API a identifiers...
        let event_request = EventRequest {
            request: EventRequestType::State(StateRequest {
                subject_id: match DigestIdentifier::from_str(&event_request.request.subject_id) {
                    Ok(subject_id) => subject_id,
                    Err(_) => {
                        return APIResponses::ExternalRequest(Err(ApiError::InvalidParameters(
                            format!("SubjectID {}", event_request.request.subject_id),
                        )))
                    }
                },
                payload: event_request.request.payload.into(),
            }),
            timestamp: event_request.timestamp,
            signature: Signature {
                content: SignatureContent {
                    signer: match KeyIdentifier::from_str(&event_request.signature.content.signer) {
                        Ok(signer) => signer,
                        Err(_) => {
                            return APIResponses::ExternalRequest(Err(ApiError::InvalidParameters(
                                format!(
                                    "Signature signer {}",
                                    event_request.signature.content.signer
                                ),
                            )))
                        }
                    },
                    event_content_hash: match DigestIdentifier::from_str(
                        &event_request.signature.content.event_content_hash,
                    ) {
                        Ok(subject_id) => subject_id,
                        Err(_) => {
                            return APIResponses::ExternalRequest(Err(ApiError::InvalidParameters(
                                format!(
                                    "Signature event content hash {}",
                                    event_request.signature.content.event_content_hash
                                ),
                            )))
                        }
                    },
                    timestamp: event_request.signature.content.timestamp,
                },
                signature: match SignatureIdentifier::from_str(&event_request.signature.signature) {
                    Ok(signature_id) => signature_id,
                    Err(_) => {
                        return APIResponses::ExternalRequest(Err(ApiError::InvalidParameters(
                            format!("Signature {}", event_request.signature.signature),
                        )))
                    }
                },
            },
            approvals: HashSet::new(),
        };
        let result = self.request_api.event_request(event_request).await;
        match result {
            Ok(result) => APIResponses::ExternalRequest(Ok(result)),
            Err(ResponseError::EventCreationError { source }) => {
                APIResponses::ExternalRequest(Err(source.into()))
            }
            Err(error) => APIResponses::ExternalRequest(Err(error.into())),
        }
    }

    pub fn get_all_subjects(&self, data: GetAllSubjects) -> APIResponses {
        let subjects = self.db.get_all_subjects();
        let (init, end) = get_init_and_end(data.from, data.quantity, &subjects);
        let result = subjects[init..end].to_owned();
        let result = result
            .into_iter()
            .filter(|subject| subject.subject_data.is_some())
            .map(|subject| subject.subject_data.unwrap())
            .collect::<Vec<SubjectData>>();
        APIResponses::GetAllSubjects(Ok(result))
    }

    pub async fn get_all_governances(&self) -> APIResponses {
        let subjects = self.db.get_all_subjects();
        let result = subjects
            .into_iter()
            .filter(|subject| {
                subject.subject_data.is_some()
                    && subject
                        .subject_data
                        .as_ref()
                        .unwrap()
                        .governance_id
                        .digest
                        .is_empty()
            })
            .map(|subject| subject.subject_data.unwrap())
            .collect::<Vec<SubjectData>>();
        APIResponses::GetAllGovernances(Ok(result))
    }

    pub async fn get_single_subject(&self, data: GetSingleSubjectAPI) -> APIResponses {
        let Ok(id) = DigestIdentifier::from_str(&data.subject_id) else {
            return APIResponses::GetSingleSubject(Err(ApiError::InvalidParameters(format!("SubjectID {}", data.subject_id))));
        };
        let Some(subject) = self.db.get_subject(&id) else {
            return APIResponses::GetSingleSubject(Err(ApiError::NotFound(format!("Subject {}", data.subject_id))))
        };
        if subject.subject_data.is_some() {
            APIResponses::GetSingleSubject(Ok(subject.subject_data.unwrap()))
        } else {
            APIResponses::GetSingleSubject(Err(ApiError::NotFound("Inner subject data".into())))
        }
    }

    pub async fn get_events_of_subject(&self, data: GetEventsOfSubject) -> APIResponses {
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
        let Ok(id) = DigestIdentifier::from_str(&data.subject_id) else {
            return APIResponses::GetEventsOfSubject(Err(ApiError::InvalidParameters(format!("SubjectID {}", data.subject_id))));
        };
        let events = self.db.get_events_by_range(&id, from, quantity);
        APIResponses::GetEventsOfSubject(Ok(events))
    }

    pub async fn get_signatures(&self, data: GetSignatures) -> APIResponses {
        let Ok(id) = DigestIdentifier::from_str(&data.subject_id) else {
            return APIResponses::GetSignatures(Err(ApiError::InvalidParameters(format!("SubjectID {}", data.subject_id))));
        };
        let Some(signatures) = self.db.get_signatures(
            &id,
            data.sn,
        ) else {
            return APIResponses::GetSignatures(Err(ApiError::NotFound(format!("Subject {} SN {}", data.subject_id, data.sn))));
        };
        let signatures = Vec::from_iter(signatures);
        let (init, end) = get_init_and_end(data.from, data.quantity, &signatures);
        let result = signatures[init..end].to_owned();
        APIResponses::GetSignatures(Ok(result))
    }

    pub async fn simulate_event(&self, data: CreateEvent) -> APIResponses {
        let Ok(id) = DigestIdentifier::from_str(&data.subject_id) else {
            return APIResponses::SimulateEvent(Err(ApiError::InvalidParameters(format!("SubjectID {}", data.subject_id))));
        };
        let request = EventRequestType::State(StateRequest {
            subject_id: id.clone(),
            payload: data.payload.clone().into(),
        });
        let Ok(signature) = self.signature_manager.sign(&request) else {
            return APIResponses::SimulateEvent(Err(ApiError::SignError));
        };
        let Some(subject) = self.db.get_subject(&id) else {
            return APIResponses::SimulateEvent(Err(ApiError::NotFound(format!("Subject {}", data.subject_id))));
        };
        let Some(subject_data) = subject.subject_data.clone() else {
            return APIResponses::SimulateEvent(Err(ApiError::NotFound(format!("Subject data of {}", data.subject_id))));
        };
        let event_request = EventRequest {
            request,
            signature,
            timestamp: Utc::now().timestamp_millis(),
            approvals: HashSet::new(),
        };
        let schema = match self
            .command_api
            .get_schema(subject_data.governance_id.clone(), subject_data.schema_id)
            .await
        {
            Ok(schema) => schema,
            Err(error) => return APIResponses::SimulateEvent(Err(error.into())),
        };
        let event_content = EventContent::new(
            id.clone(),
            event_request,
            subject_data.sn,
            DigestIdentifier::default(),
            Metadata {
                namespace: format!(""),
                governance_id: subject_data.governance_id,
                governance_version: 0,
                schema_id: format!(""),
                owner: KeyIdentifier {
                    public_key: vec![],
                    derivator: KeyDerivator::Ed25519,
                },
            },
            true,
        );
        let subject_data_result = subject.fake_apply(event_content, &schema);
        match subject_data_result {
            Ok(mut result) => {
                result.sn = result.sn + 1;
                APIResponses::SimulateEvent(Ok(result))
            }
            Err(_) => APIResponses::SimulateEvent(Err(ApiError::InternalError {
                source: ResponseError::SimulationFailed,
            })),
        }
    }

    pub async fn approval_acceptance(&self, acceptance: Acceptance, id: String) -> APIResponses {
        let Ok(id_digest) = DigestIdentifier::from_str(&id) else {
            return APIResponses::VoteResolve(Err(ApiError::InvalidParameters(format!("Request ID {}", id))));
        };
        match self
            .request_api
            .approval_resolve(acceptance, id_digest)
            .await
        {
            Ok(_) => {
                return APIResponses::VoteResolve(Ok(()));
            }
            Err(error) => match error {
                ResponseError::RequestNotFound => {
                    return APIResponses::VoteResolve(Err(ApiError::NotFound(format!(
                        "Request {}",
                        id
                    ))));
                }
                ResponseError::VoteNotNeeded => {
                    return APIResponses::VoteResolve(Err(ApiError::VoteNotNeeded(id)));
                }
                _ => {
                    return APIResponses::VoteResolve(Err(error.into()));
                }
            },
        };
    }

    pub async fn get_pending_request(&self) -> APIResponses {
        match self.request_api.get_pending_requests().await {
            Ok(data) => return APIResponses::GetPendingRequests(Ok(data)),
            Err(error) => return APIResponses::GetPendingRequests(Err(error.into())),
        }
    }

    pub async fn get_single_request(&self, id: String) -> APIResponses {
        let Ok(id_digest) = DigestIdentifier::from_str(&id) else {
            return APIResponses::GetSingleRequest(Err(ApiError::InvalidParameters(format!("Request ID {}", id))));
        };
        match self.request_api.get_single_pending_request(id_digest).await {
            Ok(data) => return APIResponses::GetSingleRequest(Ok(data)),
            Err(ResponseError::RequestNotFound) => {
                return APIResponses::GetSingleRequest(Err(ApiError::NotFound(format!(
                    "Request {}",
                    id
                ))))
            }
            Err(error) => return APIResponses::GetSingleRequest(Err(error.into())),
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
