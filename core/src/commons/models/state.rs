use crate::{
    commons::{
        crypto::KeyPair,
        errors::SubjectError,
        identifier::{DigestIdentifier, KeyIdentifier},
    },
    Derivable, signature::Signed, EventContent,
};
use borsh::{BorshDeserialize, BorshSerialize};
use json_patch::{patch, Patch};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{event_request::EventRequestType, value_wrapper::ValueWrapper};

#[derive(Debug, Deserialize, Serialize, Clone, BorshSerialize, BorshDeserialize)]
pub struct Subject {
    pub keys: Option<KeyPair>,
    /// Subject identifier
    pub subject_id: DigestIdentifier,
    /// Governance identifier
    pub governance_id: DigestIdentifier,
    /// Current sequence number of the subject
    pub sn: u64,
    pub genesis_gov_version: u64,
    /// Public key of the subject
    pub public_key: KeyIdentifier,
    pub namespace: String,
    pub name: String,
    /// Identifier of the schema used by the subject and defined in associated governance
    pub schema_id: String,
    /// Subject owner identifier
    pub owner: KeyIdentifier,
    /// Subject creator identifier
    pub creator: KeyIdentifier,
    /// Current status of the subject
    pub properties: ValueWrapper,
    /// Indicates if the subject is active or not
    pub active: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SubjectData {
    /// Subject identifier
    pub subject_id: DigestIdentifier,
    /// Governance identifier
    pub governance_id: DigestIdentifier,
    /// Current sequence number of the subject
    pub sn: u64,
    /// Public key of the subject
    pub public_key: KeyIdentifier,
    pub namespace: String,
    /// Identifier of the schema used by the subject and defined in associated governance
    pub schema_id: String,
    /// Subject owner identifier
    pub owner: KeyIdentifier,
    /// Subject creator identifier
    pub creator: KeyIdentifier,
    /// Current status of the subject
    pub properties: ValueWrapper,
    /// Indicates if the subject is active or not
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

    pub fn from_genesis_event(
        event: Signed<EventContent>,
        init_state: ValueWrapper,
        keys: Option<KeyPair>,
    ) -> Result<Self, SubjectError> {
        let EventRequestType::Create(create_request) = event.content.event_proposal.proposal.event_request.request.clone() else {
            return Err(SubjectError::NotCreateEvent)
        };
        let subject_id = generate_subject_id(
            &create_request.namespace,
            &create_request.schema_id,
            create_request.public_key.to_str(),
            create_request.governance_id.to_str(),
            event.content.event_proposal.proposal.gov_version,
        )?;
        Ok(Subject {
            keys,
            subject_id,
            governance_id: create_request.governance_id.clone(),
            sn: 0,
            public_key: create_request.public_key,
            namespace: create_request.namespace.clone(),
            schema_id: create_request.schema_id.clone(),
            owner: event
                .content
                .event_proposal
                .proposal
                .event_request
                .signature
                .signer
                .clone(),
            creator: event
                .content
                .event_proposal
                .proposal
                .event_request
                .signature
                .signer
                .clone(),
            properties: init_state,
            active: true,
            name: create_request.name,
            genesis_gov_version: event.content.event_proposal.proposal.gov_version,
        })
    }

    pub fn update_subject(
        &mut self,
        json_patch: ValueWrapper,
        new_sn: u64,
    ) -> Result<(), SubjectError> {
        let prev_properties = self.properties.as_str();
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

    pub fn get_state_hash(&self) -> Result<DigestIdentifier, SubjectError> {
        Ok(
            DigestIdentifier::from_serializable_borsh(&self.properties).map_err(|_| {
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
    ) -> Result<DigestIdentifier, SubjectError> {
        let mut subject_properties = self.properties.clone();
        let json_patch = serde_json::from_value::<Patch>(json_patch.0)
            .map_err(|_| SubjectError::CryptoError(String::from("Error parsing the json patch")))?;
        patch(&mut subject_properties.0, &json_patch).map_err(|_| {
            SubjectError::CryptoError(String::from("Error applying the json patch"))
        })?;
        Ok(
            DigestIdentifier::from_serializable_borsh(&subject_properties).map_err(|_| {
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
) -> Result<DigestIdentifier, SubjectError> {
    let subject_id = DigestIdentifier::from_serializable_borsh((
        namespace,
        schema_id,
        public_key,
        governance_id,
        governance_version,
    ))
    .map_err(|_| SubjectError::ErrorCreatingSubjectId)?;
    Ok(subject_id)
}
