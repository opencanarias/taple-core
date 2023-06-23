//! Contains the data structures related to the content of TAPLE events.
use borsh::{BorshDeserialize, BorshSerialize};

use serde::{Deserialize, Serialize};

use crate::identifier::{DigestIdentifier, KeyIdentifier};

/// Metadata of a TAPLE Event
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Metadata {
    pub namespace: String,
    pub subject_id: DigestIdentifier,
    pub governance_id: DigestIdentifier,
    pub governance_version: u64,
    pub schema_id: String,
}
