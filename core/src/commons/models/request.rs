use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{
    commons::errors::SubjectError,
    signature::{Signature, Signed},
    DigestIdentifier, KeyIdentifier, ValueWrapper,
};

use super::HashId;

/// An enum representing a TAPLE event request.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum EventRequest {
    /// A request to create a new subject.
    Create(StartRequest),
    /// A request to add a fact to a subject.
    Fact(FactRequest),
    /// A request to transfer ownership of a subject.
    Transfer(TransferRequest),
    /// A request to mark a subject as end-of-life.
    EOL(EOLRequest),
}

/// A struct representing a request to create a new subject.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct StartRequest {
    /// The identifier of the governance contract.
    pub governance_id: DigestIdentifier,
    /// The identifier of the schema used to validate the event.
    pub schema_id: String,
    /// The namespace of the subject.
    pub namespace: String,
    /// The name of the subject.
    pub name: String,
    /// The identifier of the public key of the subject owner.
    pub public_key: KeyIdentifier,
}

/// A struct representing a request to add a fact to a subject.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct FactRequest {
    /// The identifier of the subject to which the fact will be added.
    pub subject_id: DigestIdentifier,
    /// The payload of the fact to be added.
    pub payload: ValueWrapper,
}

/// A struct representing a request to transfer ownership of a subject.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct TransferRequest {
    /// The identifier of the subject to transfer ownership of.
    pub subject_id: DigestIdentifier,
    /// The identifier of the public key of the new owner.
    pub public_key: KeyIdentifier,
}

/// A struct representing a request to mark a subject as end-of-life.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct EOLRequest {
    /// The identifier of the subject to mark as end-of-life.
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
pub enum RequestState {
    Finished,
    Error,
    Processing,
}

/// A struct representing a TAPLE request.
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TapleRequest {
    /// The identifier of the request.
    pub id: DigestIdentifier,
    /// The identifier of the subject associated with the request, if any.
    pub subject_id: Option<DigestIdentifier>,
    /// The sequence number of the request, if any.
    pub sn: Option<u64>,
    /// The event request associated with the request.
    pub event_request: Signed<EventRequest>,
    /// The state of the request.
    pub state: RequestState,
    /// The success status of the request, if any.
    pub success: Option<bool>,
}

impl TryFrom<Signed<EventRequest>> for TapleRequest {
    type Error = SubjectError;

    fn try_from(event_request: Signed<EventRequest>) -> Result<Self, Self::Error> {
        let id = DigestIdentifier::from_serializable_borsh(&event_request)
            .map_err(|_| SubjectError::CryptoError("Error generation request hash".to_owned()))?;
        let subject_id = match &event_request.content {
            crate::EventRequest::Create(_) => None,
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
            success: None,
        })
    }
}
