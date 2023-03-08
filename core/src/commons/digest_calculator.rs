use blake3::Hash;
use borsh::BorshSerialize;
use crate::errors::Error;

pub fn digest_calculator<T: BorshSerialize>(serializable: T) -> Result<Hash, Error> {
    let bytes = match serializable.try_to_vec() {
        Ok(bytes) => bytes,
        Err(_) => return Err(Error::BorshSerializationFailed),
    };
    let digest = blake3::hash(&bytes);
    Ok(digest)
}
