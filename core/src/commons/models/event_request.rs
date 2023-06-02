//! Contains the data structures related to event requests.
use borsh::{BorshDeserialize, BorshSerialize};

use serde::{Deserialize, Serialize};

use crate::commons::{
    crypto::{check_cryptography, KeyGenerator},
    errors::SubjectError,
    identifier::{Derivable, DigestIdentifier, KeyIdentifier},
    schema_handler::Schema,
};
use utoipa::ToSchema;

use super::{
    signature::Signature,
    state::Subject,
    timestamp::TimeStamp,
};

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct EventRequest {
    pub request: EventRequestType,
    pub timestamp: TimeStamp,
    pub signature: Signature,
}


#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema)]
pub enum EventRequestType {
    Create(CreateRequest),
    State(StateRequest),
    Transfer(TransferRequest),
    EOL(EOLRequest),
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct CreateRequest {
    #[schema(value_type = String)]
    pub governance_id: DigestIdentifier,
    pub schema_id: String,
    pub namespace: String,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct StateRequest {
    #[schema(value_type = String)]
    pub subject_id: DigestIdentifier,
    pub invokation: String,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct TransferRequest {
    #[schema(value_type = String)]
    pub subject_id: DigestIdentifier,
    pub public_key: KeyIdentifier,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct EOLRequest {
    #[schema(value_type = String)]
    pub subject_id: DigestIdentifier,
    pub public_key: KeyIdentifier,
}

impl EventRequest {
    pub fn new(request: EventRequestType, signature: Signature) -> Self {
        Self {
            request,
            timestamp: TimeStamp::now(),
            signature,
        }
    }

    pub fn check_signatures(&self) -> Result<(), SubjectError> {
        check_cryptography((&self.request, &self.timestamp), &self.signature)
            .map_err(|error| SubjectError::CryptoError(error.to_string()))?;
        Ok(())
    }
}

// impl EventRequest {
//     pub fn check_against_schema(
//         &self,
//         schema: &Value,
//         subject: &Subject,
//     ) -> Result<(), SubjectError> {
//         let subject_schema =
//             Schema::compile(&schema).map_err(|_| SubjectError::SchemaDoesNotCompile)?;
//         let payload = match &self.request {
//             EventRequestType::State(data) => &data.payload,
//             EventRequestType::Create(data) => &data.payload,
//         };
//         match payload {
//             RequestPayload::Json(props) => {
//                 // Validate with schema
//                 let Ok(properties) = serde_json::from_str(&props) else {
//                     return Err(SubjectError::ErrorParsingJsonString);
//                 };
//                 if !subject_schema.validate(&properties) {
//                     return Err(SubjectError::SchemaValidationFailed);
//                 }
//                 Ok(())
//             }
//             RequestPayload::JsonPatch(patch_string) => {
//                 let Ok(patch_json) = serde_json::from_str(&patch_string) else {
//                     return Err(SubjectError::ErrorParsingJsonString);
//                 };
//                 let Some(subject_data) = &subject.subject_data else {
//                     return Err(SubjectError::InvalidUseOfJSONPATCH);
//                 };
//                 let Ok(mut properties) = serde_json::from_str(&subject_data.properties) else {
//                     return Err(SubjectError::ErrorParsingJsonString);
//                 };
//                 let Ok(()) = patch(&mut properties, &patch_json) else {
//                     return Err(SubjectError::ErrorApplyingPatch);
//                 };
//                 if !subject_schema.validate(&properties) {
//                     return Err(SubjectError::SchemaValidationFailed);
//                 }
//                 Ok(())
//             }
//         }
//     }

//     pub fn check_signatures(&self) -> Result<(), CryptoErrorEvent> {
//         // Checking request signature
//         let Ok(hash) = DigestIdentifier::from_serializable_borsh((self.request.clone(), self.timestamp.clone())) else {
//             return Err(CryptoErrorEvent::EventRequestHashingError);
//         };
//         // println!("Primero {:?}, segundo {:?}", hash, self.signature.content.event_content_hash);
//         if hash != self.signature.content.event_content_hash {
//             return Err(CryptoErrorEvent::EventRequestHashingConflict);
//         }
//         match self
//             .signature
//             .content
//             .signer
//             .verify(&hash.derivative(), self.signature.signature.clone())
//         {
//             Ok(_) => (),
//             Err(_) => return Err(CryptoErrorEvent::RequestSignatureInvalid),
//         };
//         for approval in self.approvals.iter() {
//             if hash != approval.content.event_request_hash {
//                 return Err(CryptoErrorEvent::EventRequestHashingConflict);
//             }
//             match DigestIdentifier::from_serializable_borsh((
//                 approval.content.event_request_hash.clone(),
//                 approval.content.approval_type.clone(),
//                 approval.content.expected_sn,
//             )) {
//                 Ok(hash) => {
//                     match approval
//                         .content
//                         .signer
//                         .verify(&hash.derivative(), approval.signature.clone())
//                     {
//                         Ok(_) => (),
//                         Err(_) => return Err(CryptoErrorEvent::RequestSignatureInvalid),
//                     }
//                 }
//                 Err(_) => return Err(CryptoErrorEvent::EventRequestHashingError),
//             }
//         }
//         Ok(())
//     }

//     pub fn create_subject_from_request(
//         self,
//         governance_version: u64,
//         subject_schema: &Value,
//         approved: bool,
//     ) -> Result<(Subject, Event), SubjectError> {
//         if let EventRequestType::Create(create_req) = self.request.clone() {
//             let mc = KeyPair::Ed25519(Ed25519KeyPair::new());
//             match DigestIdentifier::from_serializable_borsh((
//                 self.signature.content.event_content_hash.clone(),
//                 mc.public_key_bytes(),
//             )) {
//                 Err(_) => Err(SubjectError::SubjectSignatureFailed),
//                 Ok(subject_id) => {
//                     let mut event_content = EventContent::new(
//                         subject_id.clone(),
//                         self.clone(),
//                         0,
//                         DigestIdentifier::default(),
//                         Metadata {
//                             subject_id: subject_id,
//                             namespace: create_req.namespace,
//                             governance_id: create_req.governance_id,
//                             governance_version,
//                             schema_id: create_req.schema_id,
//                             owner: self.signature.content.signer,
//                         },
//                         approved,
//                     );
//                     let public_key =
//                         KeyIdentifier::new(mc.get_key_derivator(), &mc.public_key_bytes());
//                     let subject =
//                         Subject::new(&event_content, public_key, Some(mc), subject_schema)?;
//                     event_content.state_hash = subject
//                         .get_future_subject_content_hash(event_content.clone(), subject_schema)?;
//                     let event = subject.get_signature_from_subject(event_content)?;
//                     Ok((subject, event))
//                 }
//             }
//         } else {
//             Err(SubjectError::NotCreateEvent)
//         }
//     }

//     pub fn get_event_from_state_request(
//         self,
//         subject: &Subject,
//         prev_event_hash: DigestIdentifier,
//         governance_version: u64,
//         subject_schema: &Value,
//         approved: bool,
//     ) -> Result<Event, SubjectError> {
//         match self.request.clone() {
//             EventRequestType::Create(_) => {
//                 panic!("Expected State Event")
//             }
//             EventRequestType::State(state_req) => {
//                 // TODO: Check that the request invoker is you or it can be done by the governance
//                 if subject.keys.is_none() {
//                     return Err(SubjectError::NotOwnerOfSubject);
//                 }
//                 let subject_data = subject.subject_data.as_ref().expect("Hay data");
//                 let mut event_content = EventContent {
//                     subject_id: state_req.subject_id.clone(),
//                     event_request: self,
//                     sn: subject_data.sn + 1,
//                     previous_hash: prev_event_hash,
//                     state_hash: DigestIdentifier::default(),
//                     metadata: Metadata {
//                         subject_id: state_req.subject_id,
//                         namespace: subject_data.namespace.clone(),
//                         governance_id: subject_data.governance_id.clone(),
//                         governance_version,
//                         schema_id: subject_data.schema_id.clone(),
//                         owner: subject_data.owner.clone(),
//                     },
//                     approved,
//                 };
//                 event_content.state_hash = subject
//                     .get_future_subject_content_hash(event_content.clone(), subject_schema)?;
//                 Ok(subject.get_signature_from_subject(event_content)?)
//             }
//         }
//     }
// }
