use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::DigestIdentifier;

use super::errors::SubjectError;

pub mod approval;
pub mod event;
pub mod notary;
pub mod notification;
pub mod signature;
pub mod state;
pub mod timestamp;
pub mod request;
pub mod value_wrapper;
pub mod evaluation;
pub mod validation;


pub trait HashId: BorshSerialize {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError>;
}