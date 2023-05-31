use crate::{
    commons::{
        crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair, Payload, DSA},
        errors::SubjectError,
        identifier::{
            derive::KeyDerivator, Derivable, DigestIdentifier, KeyIdentifier, SignatureIdentifier,
        },
        schema_handler::{get_governance_schema, Schema},
    },
    event_request::EventRequest,
};
use json_patch::{patch, Patch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use super::{event::Event, event_request::EventRequestType};

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub struct Subject {
    pub keys: Option<KeyPair>,
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
    /// Subject creator identifier
    #[schema(value_type = String)]
    pub creator: KeyIdentifier,
    /// Current status of the subject
    pub properties: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
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
    /// Subject creator identifier
    #[schema(value_type = String)]
    pub creator: KeyIdentifier,
    /// Current status of the subject
    pub properties: String,
}

impl From<Subject> for SubjectData {
    fn from(subject: Subject) -> Self {
        Self {
            subject_id: subject.subject_id,
            governance_id: subject.governance_id,
            sn: subject.sn,
            public_key: subject.public_key,
            namespace: subject.namespace,
            schema_id: subject.schema_id,
            owner: subject.owner,
            creator: subject.creator,
            properties: subject.properties,
        }
    }
}

impl Subject {
    pub fn from_genesis_request(
        event_request: EventRequest,
        init_state: String,
    ) -> Result<Self, SubjectError> {
        let EventRequestType::Create(create_request) = event_request.request.clone() else {
            return Err(SubjectError::NotCreateEvent)
        };
        // TODO: Pasar que tipo de esquema criptográfico se quiere usar por parametros
        let keys = KeyPair::Ed25519(Ed25519KeyPair::new());
        let public_key = KeyIdentifier::new(keys.get_key_derivator(), &keys.public_key_bytes());
        let subject_id = match DigestIdentifier::from_serializable_borsh((
            &event_request.signature.content.event_content_hash,
            &public_key.public_key,
        )) {
            Ok(subject_id) => subject_id,
            Err(_) => return Err(SubjectError::ErrorCreatingSubjectId),
        };
        Ok(Subject {
            keys: Some(keys),
            subject_id,
            governance_id: create_request.governance_id.clone(),
            sn: 0,
            public_key,
            namespace: create_request.namespace.clone(),
            schema_id: create_request.schema_id.clone(),
            owner: event_request.signature.content.signer.clone(),
            creator: event_request.signature.content.signer.clone(),
            properties: init_state,
        })
    }

    pub fn from_genesis_event(event: Event, init_state: String) -> Result<Self, SubjectError> {
        let EventRequestType::Create(create_request) = event.content.event_proposal.proposal.event_request.request.clone() else {
            return Err(SubjectError::NotCreateEvent)
        };
        let subject_id = match DigestIdentifier::from_serializable_borsh((
            &event
                .content
                .event_proposal
                .proposal
                .event_request
                .signature
                .content
                .event_content_hash,
            &event.signature.content.signer.public_key,
        )) {
            Ok(subject_id) => subject_id,
            Err(_) => return Err(SubjectError::ErrorCreatingSubjectId),
        };
        Ok(Subject {
            keys: None,
            subject_id,
            governance_id: create_request.governance_id.clone(),
            sn: 0,
            public_key: event.signature.content.signer,
            namespace: create_request.namespace.clone(),
            schema_id: create_request.schema_id.clone(),
            owner: event
                .content
                .event_proposal
                .proposal
                .event_request
                .signature
                .content
                .signer
                .clone(),
            creator: event
                .content
                .event_proposal
                .proposal
                .event_request
                .signature
                .content
                .signer
                .clone(),
            properties: init_state,
        })
    }

    pub fn update_subject(&mut self, json_patch: &str, new_sn: u64) -> Result<(), SubjectError> {
        let prev_properties = self.properties.as_str();
        let Ok(patch_json) = serde_json::from_str::<Patch>(json_patch) else {
                    return Err(SubjectError::ErrorParsingJsonString(json_patch.to_owned()));
                };
        let Ok(mut state) = serde_json::from_str::<Value>(prev_properties) else {
                    return Err(SubjectError::ErrorParsingJsonString(prev_properties.to_owned()));
                };
        let Ok(()) = patch(&mut state, &patch_json) else {
                    return Err(SubjectError::ErrorApplyingPatch(json_patch.to_owned()));
                };
        let state = serde_json::to_string(&state).map_err(|_| {
            SubjectError::ErrorParsingJsonString("New State after patch".to_owned())
        })?;
        self.sn = new_sn;
        self.properties = state;
        Ok(())
    }

    pub fn transfer_subject(
        &mut self,
        owner: KeyIdentifier,
        public_key: KeyIdentifier,
        keys: Option<KeyPair>,
    ) {
        self.owner = owner;
        self.public_key = public_key;
        self.keys = keys;
    }

    pub fn get_state_hash(&self) -> Result<DigestIdentifier, SubjectError> {
        let mut subject_properties = serde_json::from_str::<Value>(&self.properties)
            .map_err(|_| SubjectError::CryptoError(String::from("Error parsing the state")))?;
        let subject_properties_str = serde_json::to_string(&subject_properties)
            .map_err(|_| SubjectError::CryptoError(String::from("Error serializing the state")))?;
        Ok(
            DigestIdentifier::from_serializable_borsh(&subject_properties_str).map_err(|_| {
                SubjectError::CryptoError(String::from("Error calculating the hash of the state"))
            })?,
        )
    }

    pub fn state_hash_after_apply(
        &self,
        json_patch: &str,
    ) -> Result<DigestIdentifier, SubjectError> {
        let mut subject_properties = serde_json::from_str::<Value>(&self.properties)
            .map_err(|_| SubjectError::CryptoError(String::from("Error parsing the state")))?;
        let json_patch = serde_json::from_str::<Patch>(json_patch)
            .map_err(|_| SubjectError::CryptoError(String::from("Error parsing the json patch")))?;
        patch(&mut subject_properties, &json_patch).map_err(|_| {
            SubjectError::CryptoError(String::from("Error applying the json patch"))
        })?;
        let subject_properties_str = serde_json::to_string(&subject_properties)
            .map_err(|_| SubjectError::CryptoError(String::from("Error serializing the state")))?;
        Ok(
            DigestIdentifier::from_serializable_borsh(&subject_properties_str).map_err(|_| {
                SubjectError::CryptoError(String::from("Error calculating the hash of the state"))
            })?,
        )
    }
}
