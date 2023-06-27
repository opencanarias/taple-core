use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::DigestIdentifier;

pub mod approval;
pub mod event;
pub mod event_content;
pub mod event_proposal;
pub mod notary;
pub mod notification;
pub mod signature;
pub mod state;
pub mod timestamp;
pub mod request;
pub mod value_wrapper;
pub mod evaluation;

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
pub enum Acceptance {
    Ok,
    Ko,
}

pub trait HashId {
    fn hash_id(&self) -> DigestIdentifier;
}