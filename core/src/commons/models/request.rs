use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::errors::SubjectError,
    signature::{Signature, Signed},
    DigestIdentifier, KeyIdentifier, ValueWrapper,
};

use super::HashId;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum EventRequest {
    Create(StartRequest),
    Fact(FactRequest),
    Transfer(TransferRequest),
    EOL(EOLRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct StartRequest {
    pub governance_id: DigestIdentifier,
    pub schema_id: String,
    pub namespace: String,
    pub name: String,
    pub public_key: KeyIdentifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct FactRequest {
    pub subject_id: DigestIdentifier,
    pub payload: ValueWrapper,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct TransferRequest {
    pub subject_id: DigestIdentifier,
    pub public_key: KeyIdentifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EOLRequest {
    pub subject_id: DigestIdentifier,
}

impl EventRequest {
    pub fn requires_eval_appr(&self) -> bool {
        match self {
            EventRequest::Fact(_) => true,
            EventRequest::Create(_) | EventRequest::Transfer(_) | EventRequest::EOL(_) => false,
        }
    }
}

impl HashId for EventRequest {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError> {
        DigestIdentifier::from_serializable_borsh(&self).map_err(|_| {
            SubjectError::SignatureCreationFails("HashId for EventRequest Fails".to_string())
        })
    }
}

impl Signed<EventRequest> {
    pub fn new(request: EventRequest, signature: Signature) -> Self {
        Self {
            content: request,
            signature,
        }
    }

    pub fn verify(&self) -> Result<(), SubjectError> {
        self.signature.verify(&self.content)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EventRequestState {
    pub id: DigestIdentifier,
    pub subject_id: Option<DigestIdentifier>,
    pub sn: Option<u64>,
    pub state: RequestState,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum RequestState {
    Finished,
    Error,
    Processing,
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TapleRequest {
    pub id: DigestIdentifier,
    pub subject_id: Option<DigestIdentifier>,
    pub sn: Option<u64>,
    pub event_request: Signed<EventRequest>,
    pub state: RequestState,
}

impl TryFrom<Signed<EventRequest>> for TapleRequest {
    type Error = SubjectError;

    fn try_from(event_request: Signed<EventRequest>) -> Result<Self, Self::Error> {
        let id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| SubjectError::CryptoError("Error generation request hash".to_owned()))?;
        let subject_id = match &event_request.content {
            crate::EventRequest::Create(create_request) => None,
            crate::EventRequest::Fact(fact_request) => Some(fact_request.subject_id.clone()),
            crate::EventRequest::Transfer(transfer_res) => Some(transfer_res.subject_id.clone()),
            crate::EventRequest::EOL(eol_request) => Some(eol_request.subject_id.clone()),
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
