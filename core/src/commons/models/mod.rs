use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

pub mod approval;
pub mod event;
pub mod event_content;
pub mod event_preevaluation;
pub mod event_proposal;
pub mod event_request;
pub mod notary;
pub mod notification;
pub mod signature;
pub mod state;
pub mod timestamp;
pub mod request;

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
