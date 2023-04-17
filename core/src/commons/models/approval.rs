//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use std::hash::Hasher;

use crate::{identifier::DigestIdentifier, signature::Signature};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use std::hash::Hash;
use super::Acceptance;

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
pub struct Approval {
    #[schema(value_type = String)]
    pub hash_event_proposal: DigestIdentifier,
    pub acceptance: Acceptance,
    pub signature: Signature,
}

impl PartialEq for Approval {
    fn eq(&self, other: &Self) -> bool {
        self.signature.content.signer == other.signature.content.signer
    }
}

impl Hash for Approval {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.signature.content.signer.hash(state);
    }
}
