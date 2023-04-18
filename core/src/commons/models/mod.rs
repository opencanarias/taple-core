use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Eq,
    PartialEq,
    BorshSerialize,
    BorshDeserialize,
    ToSchema,
    PartialOrd,
    Hash,
)]
pub enum Acceptance {
    Ok,
    Ko,
    Error,
}
