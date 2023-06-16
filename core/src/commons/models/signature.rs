//! Define the data structures related to signatures
use crate::{
    commons::errors::SubjectError,
    crypto::{KeyPair, Payload, DSA},
    identifier::{DigestIdentifier, KeyIdentifier, SignatureIdentifier},
    Derivable,
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
};

use super::timestamp::TimeStamp;

/// Defines the data used to generate the signature, as well as the signer's identifier.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    PartialOrd,
    PartialEq,
)]
pub struct SignatureContent {
    pub signer: KeyIdentifier,
    pub event_content_hash: DigestIdentifier,
    pub timestamp: TimeStamp,
}

/// The format, in addition to the signature, includes additional
/// information, namely the signer's identifier, the signature timestamp
/// and the hash of the signed contents.
#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, BorshSerialize, BorshDeserialize, PartialOrd,
)]
pub struct Signature {
    pub content: SignatureContent,
    pub signature: SignatureIdentifier,
}

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        (self.content.signer == other.content.signer)
            && (self.content.event_content_hash == other.content.event_content_hash)
    }
}

impl Hash for Signature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.content.signer.hash(state);
        self.content.event_content_hash.hash(state);
    }
}

impl Signature {
    pub fn new<T: BorshSerialize>(
        content: &T,
        signer: KeyIdentifier,
        keys: &KeyPair,
    ) -> Result<Self, SubjectError> {
        let content_hash = DigestIdentifier::from_serializable_borsh(content)
            .map_err(|_| SubjectError::SignatureCreationFails("Content hash fails".to_string()))?;
        let timestamp = TimeStamp::now();
        let signature_hash = DigestIdentifier::from_serializable_borsh((&content_hash, &timestamp))
            .map_err(|_| {
                SubjectError::SignatureCreationFails("Signature hash fails".to_string())
            })?;
        let signature = keys
            .sign(Payload::Buffer(signature_hash.derivative()))
            .map_err(|_| SubjectError::SignatureCreationFails("Keys sign fails".to_owned()))?;
        Ok(Signature {
            content: SignatureContent {
                signer: signer.clone(),
                event_content_hash: content_hash,
                timestamp,
            },
            signature: SignatureIdentifier::new(signer.to_signature_derivator(), &signature),
        })
    }
    pub fn verify(&self) -> Result<(), SubjectError> {
        let hash_signed = DigestIdentifier::from_serializable_borsh((
            &self.content.event_content_hash,
            &self.content.timestamp,
        ))
        .map_err(|_| SubjectError::SignatureVerifyFails("Signature hash fails".to_owned()))?;
        self.content
            .signer
            .verify(&hash_signed.digest, &self.signature)
            .map_err(|_| SubjectError::SignatureVerifyFails("Signature verify fails".to_owned()))
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, BorshSerialize, BorshDeserialize, PartialOrd,
)]
pub struct UniqueSignature {
    pub signature: Signature,
}

impl PartialEq for UniqueSignature {
    fn eq(&self, other: &Self) -> bool {
        self.signature.content.signer == other.signature.content.signer
    }
}

impl Hash for UniqueSignature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.signature.content.signer.hash(state);
    }
}
