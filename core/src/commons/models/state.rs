use crate::{
    commons::{
        crypto::KeyPair,
        errors::SubjectError,
        identifier::{DigestIdentifier, KeyIdentifier},
    },
    signature::Signed,
    Derivable, Event, DigestDerivator,
};
use borsh::{BorshDeserialize, BorshSerialize};
use json_patch::{patch, Patch};
use serde::{Deserialize, Serialize};

use super::{evaluation::SubjectContext, request::EventRequest, value_wrapper::ValueWrapper};

/// A struct representing a TAPLE subject.
#[derive(Debug, Deserialize, Serialize, Clone, BorshSerialize, BorshDeserialize)]
pub struct Subject {
    /// The key pair associated with the subject, if any.
    pub keys: Option<KeyPair>,
    /// The identifier of the subject.
    pub subject_id: DigestIdentifier,
    /// The identifier of the governance contract associated with the subject.
    pub governance_id: DigestIdentifier,
    /// The current sequence number of the subject.
    pub sn: u64,
    /// The version of the governance contract that created the subject.
    pub genesis_gov_version: u64,
    /// The identifier of the public key of the subject owner.
    pub public_key: KeyIdentifier,
    /// The namespace of the subject.
    pub namespace: String,
    /// The name of the subject.
    pub name: String,
    /// The identifier of the schema used to validate the subject.
    pub schema_id: String,
    /// The identifier of the public key of the subject owner.
    pub owner: KeyIdentifier,
    /// The identifier of the public key of the subject creator.
    pub creator: KeyIdentifier,
    /// The current status of the subject.
    pub properties: ValueWrapper,
    /// Indicates whether the subject is active or not.
    pub active: bool,
}

/// A struct representing the data associated with a TAPLE subject.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SubjectData {
    /// The identifier of the subject.
    pub subject_id: DigestIdentifier,
    /// The identifier of the governance contract associated with the subject.
    pub governance_id: DigestIdentifier,
    /// The current sequence number of the subject.
    pub sn: u64,
    /// The identifier of the public key of the subject owner.
    pub public_key: KeyIdentifier,
    /// The namespace of the subject.
    pub namespace: String,
    /// The name of the subject.
    pub name: String,
    /// The identifier of the schema used to validate the subject.
    pub schema_id: String,
    /// The identifier of the public key of the subject owner.
    pub owner: KeyIdentifier,
    /// The identifier of the public key of the subject creator.
    pub creator: KeyIdentifier,
    /// The current status of the subject.
    pub properties: ValueWrapper,
    /// Indicates whether the subject is active or not.
    pub active: bool,
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
            active: subject.active,
            name: subject.name,
        }
    }
}

impl Subject {
    // // TODO: Probablemente borrar
    // pub fn from_genesis_request(
    //     event_request: EventRequest,
    //     init_state: String,
    // ) -> Result<Self, SubjectError> {
    //     let EventRequestType::Create(create_request) = event_request.request.clone() else {
    //         return Err(SubjectError::NotCreateEvent)
    //     };
    //     // TODO: Pasar que tipo de esquema criptográfico se quiere usar por parametros
    //     let keys = KeyPair::Ed25519(Ed25519KeyPair::new());
    //     let public_key = KeyIdentifier::new(keys.get_key_derivator(), &keys.public_key_bytes());
    //     let subject_id = generate_subject_id(
    //         &create_request.namespace,
    //         &create_request.schema_id,
    //         create_request.public_key.to_str(),
    //         create_request.governance_id.to_str(),
    //         0, // Ta mal
    //     )?;
    //     Ok(Subject {
    //         keys: Some(keys),
    //         subject_id,
    //         governance_id: create_request.governance_id.clone(),
    //         sn: 0,
    //         public_key,
    //         namespace: create_request.namespace.clone(),
    //         schema_id: create_request.schema_id.clone(),
    //         owner: event_request.signature.content.signer.clone(),
    //         creator: event_request.signature.content.signer.clone(),
    //         properties: init_state,
    //         active: true,
    //         name: create_request.name,
    //     })
    // }

    pub fn get_subject_context(&self, invoker: KeyIdentifier) -> SubjectContext {
        SubjectContext {
            governance_id: self.governance_id.clone(),
            schema_id: self.schema_id.clone(),
            is_owner: invoker == self.owner,
            state: self.properties.clone(),
            namespace: self.namespace.clone(),
        }
    }

    pub fn from_genesis_event(
        event: Signed<Event>,
        init_state: ValueWrapper,
        keys: Option<KeyPair>,
        derivator: DigestDerivator
    ) -> Result<Self, SubjectError> {
        let EventRequest::Create(create_request) = event.content.event_request.content.clone() else {
            return Err(SubjectError::NotCreateEvent)
        };
        let subject_id = generate_subject_id(
            &create_request.namespace,
            &create_request.schema_id,
            create_request.public_key.to_str(),
            create_request.governance_id.to_str(),
            event.content.gov_version,
            derivator
        )?;
        Ok(Subject {
            keys,
            subject_id,
            governance_id: create_request.governance_id.clone(),
            sn: 0,
            public_key: create_request.public_key,
            namespace: create_request.namespace.clone(),
            schema_id: create_request.schema_id.clone(),
            owner: event.content.event_request.signature.signer.clone(),
            creator: event.content.event_request.signature.signer.clone(),
            properties: init_state,
            active: true,
            name: create_request.name,
            genesis_gov_version: event.content.gov_version,
        })
    }

    pub fn update_subject(
        &mut self,
        json_patch: ValueWrapper,
        new_sn: u64,
    ) -> Result<(), SubjectError> {
        let Ok(patch_json) = serde_json::from_value::<Patch>(json_patch.0) else {
                    return Err(SubjectError::ErrorParsingJsonString("Json Patch conversion fails".to_owned()));
                };
        let Ok(()) = patch(&mut self.properties.0, &patch_json) else {
                    return Err(SubjectError::ErrorApplyingPatch("Error Applying Patch".to_owned()));
                };
        self.sn = new_sn;
        Ok(())
    }

    pub fn transfer_subject(
        &mut self,
        owner: KeyIdentifier,
        public_key: KeyIdentifier,
        keys: Option<KeyPair>,
        sn: u64,
    ) {
        self.owner = owner;
        self.public_key = public_key;
        self.keys = keys;
        self.sn = sn;
    }

    pub fn get_state_hash(&self, derivator: DigestDerivator) -> Result<DigestIdentifier, SubjectError> {
        Ok(
            DigestIdentifier::from_serializable_borsh(&self.properties, derivator).map_err(|_| {
                SubjectError::CryptoError(String::from("Error calculating the hash of the state"))
            })?,
        )
    }

    pub fn eol_event(&mut self) {
        self.active = false;
    }

    pub fn state_hash_after_apply(
        &self,
        json_patch: ValueWrapper,
        derivator: DigestDerivator
    ) -> Result<DigestIdentifier, SubjectError> {
        let mut subject_properties = self.properties.clone();
        let json_patch = serde_json::from_value::<Patch>(json_patch.0)
            .map_err(|_| SubjectError::CryptoError(String::from("Error parsing the json patch")))?;
        patch(&mut subject_properties.0, &json_patch).map_err(|_| {
            SubjectError::CryptoError(String::from("Error applying the json patch"))
        })?;
        Ok(
            DigestIdentifier::from_serializable_borsh(&subject_properties, derivator).map_err(|_| {
                SubjectError::CryptoError(String::from("Error calculating the hash of the state"))
            })?,
        )
    }
}

pub fn generate_subject_id(
    namespace: &str,
    schema_id: &str,
    public_key: String,
    governance_id: String,
    governance_version: u64,
    derivator: DigestDerivator
) -> Result<DigestIdentifier, SubjectError> {
    let subject_id = DigestIdentifier::from_serializable_borsh((
        namespace,
        schema_id,
        public_key,
        governance_id,
        governance_version,
    ), derivator)
    .map_err(|_| SubjectError::ErrorCreatingSubjectId)?;
    Ok(subject_id)
}
