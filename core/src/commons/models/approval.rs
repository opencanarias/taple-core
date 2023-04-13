//! Contains the data structures related to event  to send to approvers, or to validators if approval is not required.
use crate::{identifier::DigestIdentifier, signature::Signature};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::Acceptance;

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema,
)]
pub struct Approval {
    #[schema(value_type = String)]
    pub hash_event_proposal: DigestIdentifier,
    pub acceptance: Acceptance,
    pub signature: Signature,
}
