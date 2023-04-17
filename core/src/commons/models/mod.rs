use serde::{Serialize, Deserialize};
use borsh::{BorshDeserialize, BorshSerialize};
use utoipa::ToSchema;

pub mod approval_signature;
pub mod event;
pub mod event_content;
pub mod event_request;
pub mod notification;
pub mod signature;
pub mod state;
pub mod timestamp;
pub mod notary;
pub mod event_preevaluation;
pub mod event_proposal;
pub mod approval;

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema, PartialOrd
)]
pub enum Acceptance {
    Ok,
    Ko,
    Error,
}
