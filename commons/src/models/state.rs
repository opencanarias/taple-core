use std::{collections::HashSet, str::FromStr};

use crate::{
    crypto::{KeyMaterial, KeyPair, Payload, DSA},
    errors::SubjectError,
    identifier::{
        derive::KeyDerivator, Derivable, DigestIdentifier, KeyIdentifier, SignatureIdentifier,
    },
    schema_handler::{get_governance_schema, Schema},
};
use time::OffsetDateTime;
use json_patch::patch;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use super::{
    event::Event,
    event_content::EventContent,
    event_request::{EventRequestType, RequestPayload},
    signature::{Signature, SignatureContent},
};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, BorshDeserialize, BorshSerialize)]
pub struct LedgerState {
    pub head_sn: Option<u64>,
    pub head_candidate_sn: Option<u64>,
    pub negociating_next: bool,
}

impl Default for LedgerState {
    fn default() -> Self {
        Self {
            head_sn: None,
            head_candidate_sn: None,
            negociating_next: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Subject {
    pub subject_data: Option<SubjectData>,
    pub keys: Option<KeyPair>,
    pub ledger_state: LedgerState,
}

/// TAPLE protocol subject data structure
#[derive(
    Debug, Clone, Eq, PartialEq, Deserialize, Serialize, BorshDeserialize, BorshSerialize, ToSchema,
)]
pub struct SubjectData {
    /// Subject identifier
    #[schema(value_type = String)]
    pub subject_id: DigestIdentifier,
    /// Governance identifier
    #[schema(value_type = String)]
    pub governance_id: DigestIdentifier,
    /// Current sequence number of the subject
    pub sn: u64,
    /// Public key of the subject
    #[schema(value_type = String)]
    pub public_key: KeyIdentifier,
    pub namespace: String,
    /// Identifier of the schema used by the subject and defined in associated governance
    pub schema_id: String,
    /// Subject owner identifier
    #[schema(value_type = String)]
    pub owner: KeyIdentifier,
    /// Current status of the subject
    pub properties: String,
}

impl Subject {
    pub fn new(
        event_content: &EventContent,
        public_key: KeyIdentifier,
        keys: Option<KeyPair>,
        subject_schema: &Value,
    ) -> Result<Self, SubjectError> {
        if event_content.sn != 0 {
            Err(SubjectError::SnNot0)
        } else if let EventRequestType::Create(ev_req) = &event_content.event_request.request {
            Ok(Self {
                subject_data: Some(SubjectData {
                    subject_id: event_content.subject_id.clone(),
                    governance_id: ev_req.governance_id.clone(),
                    sn: 0,
                    public_key,
                    namespace: ev_req.namespace.clone(),
                    schema_id: ev_req.schema_id.clone(),
                    owner: event_content.event_request.signature.content.signer.clone(),
                    properties: if let RequestPayload::Json(props) = ev_req.payload.clone() {
                        // Validate with schema
                        let properties: Value = serde_json::from_str(&props).unwrap();
                        // The schemas defined for a governance subject must be checked recursively to determine
                        // if they are correct. The check is performed during compilation.
                        let gov_schema = get_governance_schema();
                        let schema_compiled = Schema::compile(subject_schema).expect("It Compiles");
                        if !schema_compiled.validate(&properties) {
                            return Err(SubjectError::SchemaValidationFailed);
                        }
                        let mut member_ids: HashSet<String> = HashSet::new();
                        let mut schema_ids: HashSet<String> = HashSet::new();
                        schema_ids.insert(String::from("governance"));
                        if &gov_schema == subject_schema {
                            // It is governance: checking subschemas
                            // Properties validation
                            let schemas = properties
                                .get("schemas")
                                .unwrap()
                                .as_array()
                                .unwrap()
                                .to_owned();
                            for schema in schemas.into_iter() {
                                if let Err(_) = Schema::compile(&schema) {
                                    return Err(SubjectError::SchemaValidationFailed);
                                }
                                let schema_id = schema
                                    .get("id")
                                    .expect("Se ha validado que tiene id")
                                    .as_str()
                                    .expect("Hay id y es str")
                                    .to_owned();
                                let true = schema_ids.insert(schema_id) else {
                                    return Err(SubjectError::DuplicatedSchemaOrMember);
                                };
                            }
                            let members = properties
                                .get("members")
                                .unwrap()
                                .as_array()
                                .unwrap()
                                .to_owned();
                            for member in members.into_iter() {
                                let member_id = member
                                    .get("key")
                                    .expect("Se ha validado que tiene key")
                                    .as_str()
                                    .expect("Hay id y es str")
                                    .to_owned();
                                // Check if the member ID is valid
                                let Ok(_) = KeyIdentifier::from_str(&member_id) else {
                                    return Err(SubjectError::InvalidMemberIdentifier(member_id.to_owned()))
                                };
                                let true = member_ids.insert(member_id) else {
                                    return Err(SubjectError::DuplicatedSchemaOrMember);
                                };
                            }
                            // Validate Policies
                            let policies = properties
                                .get("policies")
                                .unwrap()
                                .as_array()
                                .unwrap()
                                .to_owned();
                            for policie in policies.into_iter() {
                                // Validating policy id
                                let policie_id = policie
                                    .get("id")
                                    .expect("Se ha validado que tiene id")
                                    .as_str()
                                    .expect("Hay id y es str")
                                    .to_owned();
                                let true = schema_ids.remove(&policie_id) else {
                                    return Err(SubjectError::InvalidPoliciesId);
                                };
                                // Validating validation
                                let validators = policie
                                    .get("validation")
                                    .unwrap()
                                    .get("validators")
                                    .unwrap()
                                    .as_array()
                                    .unwrap()
                                    .to_owned();
                                let mut validator_set = HashSet::new();
                                for validator in validators.into_iter() {
                                    let validator = validator.as_str().expect("Has to be");
                                    validator_set.insert(String::from(validator));
                                    let true = member_ids.contains(validator) else {
                                        return Err(SubjectError::InvalidMemberInPolicies);
                                    };
                                }
                                let approvers = policie
                                    .get("approval")
                                    .unwrap()
                                    .get("approvers")
                                    .unwrap()
                                    .as_array()
                                    .unwrap()
                                    .to_owned();
                                let mut approver_set = HashSet::new();
                                for approver in approvers.into_iter() {
                                    let approver = approver.as_str().expect("Has to be");
                                    approver_set.insert(String::from(approver));
                                    let true = member_ids.contains(approver) else {
                                        return Err(SubjectError::InvalidMemberInPolicies);
                                    };
                                }
                                // All approvers must be validators
                                if !approver_set.is_subset(&validator_set) {
                                    return Err(SubjectError::ApproversAreNotValidators);
                                }
                                let invokers = policie
                                    .get("invokation")
                                    .unwrap()
                                    .get("set")
                                    .unwrap()
                                    .get("invokers")
                                    .unwrap()
                                    .as_array()
                                    .unwrap()
                                    .to_owned();
                                for invoker in invokers.into_iter() {
                                    let invoker = invoker.as_str().expect("Has to be");
                                    let true = member_ids.contains(invoker) else {
                                        return Err(SubjectError::InvalidMemberInPolicies);
                                    };
                                }
                            }
                            if !schema_ids.is_empty() {
                                return Err(SubjectError::PoliciesMissing);
                            }
                        }
                        props
                    } else {
                        return Err(SubjectError::InvalidPayload(String::from(
                            "Payload different from Json in Genesis Event",
                        )));
                    },
                }),
                keys,
                ledger_state: LedgerState {
                    head_sn: Some(0),
                    head_candidate_sn: None,
                    negociating_next: false,
                },
            })
        } else {
            Err(SubjectError::NotCreateEvent)
        }
    }

    pub fn new_empty(ledger_state: LedgerState) -> Self {
        Self {
            subject_data: None,
            keys: None,
            ledger_state,
        }
    }

    pub fn apply(&mut self, event_content: EventContent) -> Result<(), SubjectError> {
        match self.ledger_state.head_sn {
            Some(0) => (),
            Some(head_sn) => {
                if event_content.sn != head_sn + 1 {
                    return Err(SubjectError::EventSourcingNotInOrder(
                        head_sn,
                        event_content.sn,
                    ));
                }
            }
            None => return Err(SubjectError::ApplyInEmptySubject),
        }
        let mut subject_data = {
            match self.subject_data.clone() {
                Some(sd) => sd,
                None => return Err(SubjectError::SubjectHasNoData),
            }
        };
        subject_data.properties = match event_content.event_request.request {
            EventRequestType::Create(_) => {
                // It should never be create, because it creates the subject, it does not make an event sourcing, in that case it is looked at in the new
                unreachable!("Haciendo apply en create event");
            }
            EventRequestType::State(req) => match req.payload {
                RequestPayload::Json(props) => {
                    if event_content.approved {
                        props
                    } else {
                        subject_data.properties
                    }
                }
                RequestPayload::JsonPatch(patch_string) => {
                    if event_content.approved {
                        let Ok(patch_json) = serde_json::from_str(&patch_string) else {
                        return Err(SubjectError::ErrorParsingJsonString);
                    };
                        let Ok(mut properties) =
                        serde_json::from_str(&subject_data.properties) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        let Ok(()) = patch(&mut properties, &patch_json) else {
                        return Err(SubjectError::ErrorApplyingPatch);
                    };
                        let Ok(result) = serde_json::to_string(&properties) else {
                        return Err(SubjectError::ErrorParsingJsonString);
                    };
                        result
                    } else {
                        subject_data.properties
                    }
                }
            },
        };
        subject_data.sn = event_content.sn;
        // Compare hash of event status with the hash resulting from subjectData if you are not just getting the hash
        match DigestIdentifier::from_serializable_borsh(subject_data.clone()) {
            Ok(hash) => {
                if hash != event_content.state_hash {
                    return Err(SubjectError::EventSourcingHashNotEqual);
                }
            }
            Err(_) => return Err(SubjectError::EventContentSerializationFailed),
        };
        self.subject_data = Some(subject_data);
        self.ledger_state.negociating_next = false;
        self.ledger_state.head_sn = Some(event_content.sn);
        if self.ledger_state.head_candidate_sn.is_some()
            && self.ledger_state.head_candidate_sn.unwrap() == event_content.sn
        {
            self.ledger_state.head_candidate_sn = None;
        }
        Ok(())
    }

    pub fn get_sn(&self) -> u64 {
        self.subject_data.as_ref().expect("Hay subject").sn
    }

    pub fn get_signature_from_subject(
        &self,
        event_content: EventContent,
    ) -> Result<Event, SubjectError> {
        if self.keys.is_none() {
            return Err(SubjectError::NotOwnerOfSubject);
        }
        match DigestIdentifier::from_serializable_borsh(event_content.clone()) {
            Err(_) => Err(SubjectError::EventContentSerializationFailed),
            Ok(event_content_hash) => {
                let signature = match self
                    .keys
                    .as_ref()
                    .unwrap()
                    .sign(Payload::Buffer(event_content_hash.derivative()))
                {
                    Ok(sig) => sig,
                    Err(_) => return Err(SubjectError::SubjectSignatureFailed),
                };
                let signature = SignatureIdentifier::new(
                    self.subject_data
                        .as_ref()
                        .expect("Hay Subject Data")
                        .public_key
                        .to_signature_derivator(),
                    &signature,
                );
                let signer = KeyIdentifier::new(
                    KeyDerivator::Ed25519,
                    &self.keys.as_ref().unwrap().public_key_bytes(),
                );
                let att_signature = Signature {
                    content: SignatureContent {
                        signer: signer.clone(),
                        event_content_hash,
                        timestamp: OffsetDateTime::now_utc().unix_timestamp(),
                    },
                    signature: signature,
                };
                match Event::new(event_content, att_signature) {
                    Ok(event) => Ok(event),
                    Err(e) => Err(SubjectError::CryptoError(e)),
                }
            }
        }
    }

    pub fn get_future_subject_content_hash(
        &self,
        event_content: EventContent,
        subject_schema: &Value,
    ) -> Result<DigestIdentifier, SubjectError> {
        let subject = self.clone();
        let sd = subject.fake_apply(event_content, subject_schema)?;
        match DigestIdentifier::from_serializable_borsh(sd) {
            Ok(hash) => Ok(hash),
            Err(_) => Err(SubjectError::EventContentSerializationFailed),
        }
    }

    pub fn fake_apply(
        self,
        event_content: EventContent,
        subject_schema: &Value,
    ) -> Result<SubjectData, SubjectError> {
        match self.ledger_state.head_sn {
            Some(0) => (),
            Some(head_sn) => {
                if event_content.sn != head_sn + 1 {
                    return Err(SubjectError::EventSourcingNotInOrder(
                        head_sn,
                        event_content.sn,
                    ));
                }
            }
            None => return Err(SubjectError::ApplyInEmptySubject),
        }
        let mut subject_data = {
            match self.subject_data.clone() {
                Some(sd) => sd,
                None => return Err(SubjectError::SubjectHasNoData),
            }
        };
        subject_data.properties = match event_content.event_request.request {
            EventRequestType::Create(req) => {
                match req.payload {
                    RequestPayload::Json(props) => {
                        // Validate with schema
                        let Ok(properties) =
                        serde_json::from_str(&props) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        let Ok(subject_schema) = Schema::compile(subject_schema) else {
                            return Err(SubjectError::SchemaDoesNotCompile);
                        };
                        if !subject_schema.validate(&properties) {
                            return Err(SubjectError::SchemaValidationFailed);
                        }
                        if event_content.approved {
                            props
                        } else {
                            subject_data.properties
                        }
                    }
                    RequestPayload::JsonPatch(patch_string) => {
                        let Ok(patch_json) = serde_json::from_str(&patch_string) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        let Ok(subject_schema) = Schema::compile(subject_schema) else {
                            return Err(SubjectError::SchemaDoesNotCompile);
                        };
                        let Ok(mut properties) =
                        serde_json::from_str(&subject_data.properties) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        let Ok(()) = patch(&mut properties, &patch_json) else {
                            return Err(SubjectError::ErrorApplyingPatch);
                        };
                        if !subject_schema.validate(&properties) {
                            return Err(SubjectError::SchemaValidationFailed);
                        }
                        let Ok(result) = serde_json::to_string(&properties) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        if event_content.approved {
                            result
                        } else {
                            subject_data.properties
                        }
                    }
                }
            }
            EventRequestType::State(req) => {
                match req.payload {
                    RequestPayload::Json(props) => {
                        // Validate with schema
                        let Ok(properties) =
                        serde_json::from_str(&props) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        let Ok(subject_schema) = Schema::compile(subject_schema) else {
                            return Err(SubjectError::SchemaDoesNotCompile);
                        };
                        if !subject_schema.validate(&properties) {
                            return Err(SubjectError::SchemaValidationFailed);
                        }
                        if event_content.approved {
                            props
                        } else {
                            subject_data.properties
                        }
                    }
                    RequestPayload::JsonPatch(patch_string) => {
                        let Ok(patch_json) = serde_json::from_str(&patch_string) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        let Ok(subject_schema) = Schema::compile(subject_schema) else {
                            return Err(SubjectError::SchemaDoesNotCompile);
                        };
                        let Ok(mut properties) =
                        serde_json::from_str(&subject_data.properties) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        let Ok(()) = patch(&mut properties, &patch_json) else {
                            return Err(SubjectError::ErrorApplyingPatch);
                        };
                        if !subject_schema.validate(&properties) {
                            return Err(SubjectError::SchemaValidationFailed);
                        }
                        let Ok(result) = serde_json::to_string(&properties) else {
                            return Err(SubjectError::ErrorParsingJsonString);
                        };
                        if event_content.approved {
                            result
                        } else {
                            subject_data.properties
                        }
                    }
                }
            }
        };
        subject_data.sn = event_content.sn;
        Ok(subject_data)
    }
}

impl PartialEq for Subject {
    fn eq(&self, other: &Self) -> bool {
        self.subject_data == other.subject_data
            && (self.keys.is_none()
                || (self.keys.is_some()
                    && self.keys.as_ref().unwrap().public_key_bytes()
                        == other.keys.as_ref().unwrap().public_key_bytes()))
            && self.ledger_state == other.ledger_state
    }
}
