// use std::{collections::HashSet, str::FromStr};

// use borsh::{BorshDeserialize, BorshSerialize};
// use serde::{
//     de::{self, SeqAccess, Visitor},
//     Deserialize, Serialize,
// };
// use utoipa::ToSchema;

// use crate::commons::{
//     errors::{CryptoErrorEvent, Error},
//     identifier::{
//         Derivable, DigestIdentifier, KeyIdentifier, SignatureIdentifier,
//     },
// };

// use super::{
//     event_content::{EventContent, Metadata},
//     event_request::{EventRequest, EventRequestType, RequestPayload, StateRequest},
//     signature::{Signature, SignatureContent}, timestamp::TimeStamp,
// };

// /// Event associated to a subject.
// #[derive(Debug, Clone, Serialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema)]
// pub struct Event {
//     pub event_content: EventContent,
//     pub signature: Signature,
// }

// impl Event {
//     pub fn new(
//         event_content: EventContent,
//         signature: Signature,
//     ) -> Result<Self, CryptoErrorEvent> {
//         let event = Self {
//             event_content,
//             signature,
//         };
//         event.check_signatures()?;
//         Ok(event)
//     }

//     pub fn get_event_content_hash(&self) -> Result<DigestIdentifier, Error> {
//         DigestIdentifier::from_serializable_borsh(self.event_content.clone())
//     }

//     pub fn check_signatures(&self) -> Result<(), CryptoErrorEvent> {
//         self.event_content.event_request.check_signatures()?;
//         match DigestIdentifier::from_serializable_borsh(self.event_content.clone()) {
//             Ok(hash) => {
//                 if hash != self.signature.content.event_content_hash {
//                     return Err(CryptoErrorEvent::EventContentHashingConflict);
//                 }
//                 match &self
//                     .signature
//                     .content
//                     .signer
//                     .verify(&hash.derivative(), self.signature.signature.clone())
//                 {
//                     Ok(_) => (),
//                     Err(_) => return Err(CryptoErrorEvent::RequestSignatureInvalid),
//                 }
//             }
//             Err(_) => return Err(CryptoErrorEvent::EventContentHashingError),
//         }
//         Ok(())
//     }
// }

// impl<'de> Deserialize<'de> for Event {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: serde::Deserializer<'de>,
//     {
//         #[derive(Deserialize)]
//         #[serde(field_identifier)]
//         enum Field {
//             #[serde(rename = "event_content")]
//             EventContent,
//             #[serde(rename = "signature")]
//             Signature,
//         }

//         struct EventVisitor;
//         impl<'de> Visitor<'de> for EventVisitor {
//             type Value = Event;

//             fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
//                 formatter.write_str("struct Event")
//             }

//             fn visit_seq<V>(self, mut seq: V) -> Result<Event, V::Error>
//             where
//                 V: SeqAccess<'de>,
//             {
//                 let event_content = seq
//                     .next_element()?
//                     .ok_or_else(|| de::Error::invalid_length(0, &self))?;
//                 let signature = seq
//                     .next_element()?
//                     .ok_or_else(|| de::Error::invalid_length(1, &self))?;
//                 match Event::new(event_content, signature) {
//                     Ok(event) => Ok(event),
//                     Err(_) => Err(de::Error::custom("Signatures of event are not valid")),
//                 }
//             }

//             fn visit_map<V>(self, mut map: V) -> Result<Event, V::Error>
//             where
//                 V: de::MapAccess<'de>,
//             {
//                 let mut event_content = None;
//                 let mut signature = None;
//                 while let Some(key) = map.next_key()? {
//                     match key {
//                         Field::EventContent => {
//                             if event_content.is_some() {
//                                 return Err(de::Error::duplicate_field("event_content"));
//                             }
//                             event_content = Some(map.next_value()?);
//                         }
//                         Field::Signature => {
//                             if signature.is_some() {
//                                 return Err(de::Error::duplicate_field("signature"));
//                             }
//                             signature = Some(map.next_value()?);
//                         }
//                     }
//                 }
//                 let event_content =
//                     event_content.ok_or_else(|| de::Error::missing_field("event_content"))?;
//                 let signature = signature.ok_or_else(|| de::Error::missing_field("signature"))?;
//                 match Event::new(event_content, signature) {
//                     Ok(event) => Ok(event),
//                     Err(_) => Err(de::Error::custom("Signatures of event are not valid")),
//                 }
//             }
//         }

//         const FIELDS: &'static [&'static str] = &["event_content", "signature"];
//         deserializer.deserialize_struct("Event", FIELDS, EventVisitor)
//     }
// }

// impl Default for Event {
//     fn default() -> Self {
//         Self {
//             event_content:  EventContent {
//                 subject_id: DigestIdentifier::from_str("Ju536BiUXBqbuNdJsOBwYWnbzrKjsYtVEauI6IsMh3tM").unwrap(),
//                 event_request: EventRequest {
//                     request: EventRequestType::State(StateRequest {
//                         subject_id: DigestIdentifier::from_str("Ju536BiUXBqbuNdJsOBwYWnbzrKjsYtVEauI6IsMh3tM").unwrap(),
//                         payload: RequestPayload::Json("{\"localizacion\":\"Argentina\",\"temperatura\":-2}".to_owned()),
//                     }),
//                     timestamp: TimeStamp::now(),
//                     signature: Signature {
//                         content: SignatureContent {
//                         signer: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y").unwrap(),
//                         event_content_hash: DigestIdentifier::from_str("Jnb4JtdYKZVyX1tFCCVXJ71X-badXlPnxYJ9xe5wzrCs").unwrap(),
//                         timestamp: TimeStamp::now(),
//                         },
//                         signature: SignatureIdentifier::from_str("SErazBOSVMRgEc89jp5Xr3IT2T5D3Y_BhiyBV-Wq8HIujTjWDPPkHL6xoLYDWQu0MWnzVZ24O_dXmOEf9AxwxeDw").unwrap()
//                     },
//                     approvals: HashSet::new()
//                 },
//                 sn: 1,
//                 previous_hash: DigestIdentifier::from_str("Js9yM-ALHzBPi0pLWKzVZ7Zx8XUI3L2Wk0Wawt4hHpac").unwrap(),
//                 state_hash: DigestIdentifier::from_str("JHoBQ4IHh2QsnjvXBOs76T5Us9nwdMnNwiKn4rukMkNg").unwrap(), metadata: Metadata {
//                     namespace: "namespace1".to_owned(),
//                     governance_id: DigestIdentifier::from_str("J3pDuDQICA7iSCGDKUfIr2rconDPQ11jCKJhLUrSPM_U").unwrap(),
//                     governance_version: 0,
//                     schema_id: "Prueba".to_owned(),
//                     owner: KeyIdentifier::from_str("ED8MpwKh3OjPEw_hQdqJixrXlKzpVzdvHf2DqrPvdz7Y").unwrap(),
//                     subject_id: DigestIdentifier::from_str("J3pDuDQICA7iSCGDKUfIr2rconDPQ11jCKJhLUrSPM_U").unwrap(), }, approved: true
//                 },
//             signature: Signature {
//                 content: SignatureContent {
//                     signer: KeyIdentifier::from_str("E3jPA10tf8YGtyQJ5l0COJA-woXyBmlfGE-AbFVmZvr4").unwrap(),
//                     event_content_hash: DigestIdentifier::from_str("JvWXIptlBC_3Ybx0cTY3X-mL922Q0Ot8Jnl3inmHmsAA").unwrap(),
//                     timestamp: TimeStamp::now(),
//                 },
//                 signature: SignatureIdentifier::from_str("SEtLpVCrClCzaRZNTJ98dEOkvYi6azvBKMBgwHbkqZkDW7CSVNWjpJFg2jCROTrrJEXXrxVhqmZeBdsYEXuXkPAA").unwrap(),
//             },
//         }
//     }
// }
//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::collections::HashSet;

use crate::{
    commons::{
        crypto::{check_cryptography, KeyPair, Payload, DSA, KeyMaterial},
        errors::SubjectError,
    },
    event_content::Metadata,
    event_request::EventRequest,
    identifier::{DigestIdentifier, KeyIdentifier, SignatureIdentifier, Derivable},
    signature::{Signature, SignatureContent},
    TimeStamp,
};
use borsh::{BorshDeserialize, BorshSerialize};
use json_patch::{diff, Patch};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::ToSchema;

use super::{
    approval::Approval,
    event_proposal::{EventProposal, Proposal},
};

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct Event {
    pub content: EventContent,
    pub signature: Signature,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct EventContent {
    pub event_proposal: EventProposal,
    pub approvals: HashSet<Approval>,
    pub execution: bool,
}

impl EventContent {
    pub fn new(
        event_proposal: EventProposal,
        approvals: HashSet<Approval>,
        execution: bool,
    ) -> Self {
        Self {
            event_proposal,
            approvals,
            execution,
        }
    }
}

impl Event {
    pub fn from_genesis_request(
        event_request: EventRequest,
        subject_keys: KeyPair,
        gov_version: u64,
        init_state: &Value,
    ) -> Result<Self, SubjectError> {
        let json_patch = serde_json::to_string(&serde_json::to_value(diff(&json!({}), init_state)).map_err(|_| {
            SubjectError::CryptoError(String::from("Error converting patch to value"))
        })?).map_err(|_| {
            SubjectError::CryptoError(String::from("Error converting patch to string"))
        })?;
        let proposal = Proposal {
            event_request,
            sn: 0,
            hash_prev_event: DigestIdentifier::default(),
            gov_version,
            evaluation: None,
            json_patch,
            evaluation_signatures: HashSet::new(),
        };
        let public_key = KeyIdentifier::new(
            subject_keys.get_key_derivator(),
            &subject_keys.public_key_bytes(),
        );
        let proposal_hash = DigestIdentifier::from_serializable_borsh(&proposal).map_err(|_| {
            SubjectError::CryptoError(String::from("Error calculating the hash of the proposal"))
        })?;
        let subject_signature = subject_keys
            .sign(Payload::Buffer(proposal_hash.derivative()))
            .map_err(|_| {
                SubjectError::CryptoError(String::from("Error signing the hash of the proposal"))
            })?;
        let subject_signature = Signature {
            content: SignatureContent {
                signer: public_key.clone(),
                event_content_hash: proposal_hash.clone(),
                timestamp: TimeStamp::now(),
            },
            signature: SignatureIdentifier::new(
                public_key.to_signature_derivator(),
                &subject_signature,
            ),
        };
        let event_proposal = EventProposal::new(proposal, subject_signature);
        let content = EventContent {
            event_proposal,
            approvals: HashSet::new(),
            execution: true,
        };
        let content_hash = DigestIdentifier::from_serializable_borsh(&content).map_err(|_| {
            SubjectError::CryptoError(String::from("Error calculating the hash of the proposal"))
        })?;
        let signature = subject_keys
            .sign(Payload::Buffer(content_hash.derivative()))
            .map_err(|_| {
                SubjectError::CryptoError(String::from("Error signing the hash of the proposal"))
            })?;
        let subject_signature = Signature {
            content: SignatureContent {
                signer: public_key.clone(),
                event_content_hash: content_hash,
                timestamp: TimeStamp::now(),
            },
            signature: SignatureIdentifier::new(
                public_key.to_signature_derivator(),
                &signature,
            ),
        };
        Ok(Self { content, signature: subject_signature })
    }

    pub fn check_signatures(&self) -> Result<(), SubjectError> {
        check_cryptography(&self.content, &self.signature)
            .map_err(|error| SubjectError::CryptoError(error.to_string()))?;
        self.content.event_proposal.check_signatures()?;
        Ok(())
    }
}
