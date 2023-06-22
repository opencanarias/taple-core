use borsh::{BorshDeserialize, BorshSerialize};

use crate::Error;

pub(crate) mod approvals;
pub(crate) mod contract;
pub(crate) mod controller_id;
pub(crate) mod event;
pub(crate) mod event_request;
pub(crate) mod keys;
pub(crate) mod lce_validation_proofs;
pub(crate) mod notary;
pub(crate) mod preauthorized_subjects_and_providers;
pub(crate) mod prevalidated_event;
pub(crate) mod request;
pub(crate) mod signature;
pub(crate) mod subject;
pub(crate) mod subject_by_governance;
pub(crate) mod witness_signatures;

mod utils;

pub fn serialize<T: BorshSerialize>(data: &T) -> Result<Vec<u8>, Error> {
    data.try_to_vec().map_err(|_| Error::SerializeError)
}

pub fn deserialize<T: BorshDeserialize>(data: &[u8]) -> Result<T, Error> {
    T::try_from_slice(data).map_err(|_| Error::DeSerializeError)
}
