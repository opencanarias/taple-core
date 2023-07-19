//! Define the data structures related to signatures
use crate::{
    commons::errors::SubjectError,
    crypto::{KeyPair, Payload, DSA},
    identifier::{DigestIdentifier, KeyIdentifier, SignatureIdentifier},
    Derivable,
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

use super::{timestamp::TimeStamp, HashId};

/// Defines the data used to generate the signature, as well as the signer's identifier.
// #[derive(
//     Debug,
//     Clone,
//     Serialize,
//     Deserialize,
//     Eq,
//     BorshSerialize,
//     BorshDeserialize,
//     PartialOrd,
//     PartialEq,
// )]
// pub struct SignatureContent {
//     pub signer: KeyIdentifier,
//     pub event_content_hash: DigestIdentifier,
// }

/// The format, in addition to the signature, includes additional
/// information, namely the signer's identifier, the signature timestamp
/// and the hash of the signed contents.
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
    Hash,
)]
pub struct Signature {
    pub signer: KeyIdentifier,
    pub timestamp: TimeStamp,
    pub value: SignatureIdentifier,
}

impl Signature {
    pub fn new<T: HashId>(
        content: &T,
        signer: KeyIdentifier,
        keys: &KeyPair,
    ) -> Result<Self, SubjectError> {
        let timestamp = TimeStamp::now();
        let content_hash = content.hash_id()?;
        let signature_hash = DigestIdentifier::from_serializable_borsh((&content_hash, &timestamp))
            .map_err(|_| {
                SubjectError::SignatureCreationFails("Signature hash fails".to_string())
            })?;
        let signature = keys
            .sign(Payload::Buffer(signature_hash.derivative()))
            .map_err(|_| SubjectError::SignatureCreationFails("Keys sign fails".to_owned()))?;
        Ok(Signature {
            signer: signer.clone(),
            timestamp,
            value: SignatureIdentifier::new(signer.to_signature_derivator(), &signature),
        })
    }
    pub fn verify<T: HashId>(&self, content: &T) -> Result<(), SubjectError> {
        let content_hash = content.hash_id()?;
        let signature_hash =
            DigestIdentifier::from_serializable_borsh((&content_hash, &self.timestamp)).map_err(
                |_| SubjectError::SignatureCreationFails("Signature hash fails".to_string()),
            )?;
        self.signer
            .verify(&signature_hash.digest, &self.value)
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
        self.signature.signer == other.signature.signer
    }
}

impl Hash for UniqueSignature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.signature.signer.hash(state);
    }
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Eq,
    PartialEq,
    BorshSerialize,
    BorshDeserialize,
    PartialOrd,
    Hash,
)]
pub struct Signed<T: BorshSerialize + BorshDeserialize + Clone> {
    #[serde(flatten)]
    pub content: T,
    pub signature: Signature,
}
