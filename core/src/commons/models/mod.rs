use borsh::BorshSerialize;

use crate::DigestIdentifier;

use super::errors::SubjectError;

pub mod approval;
pub mod evaluation;
pub mod event;
pub mod notification;
pub mod request;
pub mod signature;
pub mod state;
pub mod timestamp;
pub mod validation;
pub mod value_wrapper;

pub trait HashId: BorshSerialize {
    fn hash_id(&self) -> Result<DigestIdentifier, SubjectError>;
}
