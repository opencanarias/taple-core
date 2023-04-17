//! Define the data structures related to signatures
use crate::identifier::{DigestIdentifier, KeyIdentifier, SignatureIdentifier};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::{hash::{Hash, Hasher}, collections::HashSet};
use utoipa::ToSchema;

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
    ToSchema,
    PartialEq,
)]
pub struct SignatureContent {
    #[schema(value_type = String)]
    pub signer: KeyIdentifier,
    #[schema(value_type = String)]
    pub event_content_hash: DigestIdentifier,
    pub timestamp: TimeStamp,
}

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
    ToSchema,
    PartialOrd,
)]
pub struct Signature {
    pub content: SignatureContent,
    #[schema(value_type = String)]
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

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, BorshSerialize, BorshDeserialize, ToSchema, PartialOrd,
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
