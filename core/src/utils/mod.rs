use borsh::{BorshDeserialize, BorshSerialize};

use crate::Error;
pub mod message;
pub mod patch;

pub fn serialize<T: BorshSerialize>(data: &T) -> Result<Vec<u8>, Error> {
    data.try_to_vec().map_err(|_| Error::SerializeError)
}

pub fn deserialize<T: BorshDeserialize>(data: &[u8]) -> Result<T, Error> {
    T::try_from_slice(data).map_err(|_| Error::DeSerializeError)
}
