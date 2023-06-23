use borsh::{BorshSerialize, BorshDeserialize};
use serde::{Deserialize, Serialize};

use crate::{commons::errors::SubjectError, DigestIdentifier, signature::Signed, EventRequestType};

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TapleRequest {
    pub id: DigestIdentifier,
    pub subject_id: Option<DigestIdentifier>,
    pub sn: Option<u64>,
    pub event_request: Signed<EventRequestType>,
    pub state: RequestState,
}

impl TryFrom<Signed<EventRequestType>> for TapleRequest {
    type Error = SubjectError;

    fn try_from(event_request: Signed<EventRequestType>) -> Result<Self, Self::Error> {
        let id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| SubjectError::CryptoError("Error generation request hash".to_owned()))?;
        let subject_id = match &event_request.content {
            crate::EventRequestType::Create(create_request) => None,
            crate::EventRequestType::Fact(fact_request) => Some(fact_request.subject_id.clone()),
            crate::EventRequestType::Transfer(transfer_res) => {
                Some(transfer_res.subject_id.clone())
            }
            crate::EventRequestType::EOL(eol_request) => Some(eol_request.subject_id.clone()),
        };
        Ok(Self {
            id,
            subject_id,
            sn: None,
            event_request,
            state: RequestState::Processing,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum RequestState {
    Finished,
    Error,
    Processing,
}
