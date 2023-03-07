//! x25519
//!

use base64::decode_config;
use serde::{de::Deserializer, Deserialize, Serialize, Serializer};
use std::convert::TryInto;

use super::{create_seed, BaseKeyPair, KeyGenerator, KeyMaterial, Payload, DHKE, DSA};
use crate::identifier;
use identifier::error::Error;

use x25519_dalek::{PublicKey, StaticSecret};

const KEYPAIR_LENGTH: usize = 64;
const SECRET_KEY_LENGTH: usize = 32;

pub type X25519KeyPair = BaseKeyPair<PublicKey, StaticSecret>;

impl KeyGenerator for X25519KeyPair {
    fn from_seed(seed: &[u8]) -> Self {
        // TODO: Analyze whether a modification is necessary based on how the KeyPair is created
        let secret_seed = create_seed(seed).expect("invalid seed");

        let sk = StaticSecret::from(secret_seed);
        let pk: PublicKey = (&sk).try_into().expect("invalid public key");

        X25519KeyPair {
            public_key: pk,
            secret_key: Some(sk),
        }
    }

    fn from_public_key(public_key: &[u8]) -> Self {
        let mut pk: [u8; 32] = [0; 32];
        pk.clone_from_slice(public_key);

        X25519KeyPair {
            public_key: PublicKey::from(pk),
            secret_key: None,
        }
    }

    fn from_secret_key(secret_key: &[u8]) -> Self {
        let sized_data: [u8; 32] = clone_into_array(&secret_key[..32]);

        let sk = StaticSecret::from(sized_data);
        let pk: PublicKey = (&sk).try_into().expect("invalid public key");

        X25519KeyPair {
            public_key: pk,
            secret_key: Some(sk),
        }
    }
}

impl KeyMaterial for X25519KeyPair {
    fn public_key_bytes(&self) -> Vec<u8> {
        self.public_key.to_bytes().to_vec()
    }

    fn secret_key_bytes(&self) -> Vec<u8> {
        self.secret_key.as_ref().unwrap().to_bytes().to_vec()
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes: [u8; KEYPAIR_LENGTH] = [0u8; KEYPAIR_LENGTH];
        bytes[..SECRET_KEY_LENGTH].copy_from_slice(&self.secret_key_bytes());
        bytes[SECRET_KEY_LENGTH..].copy_from_slice(&self.public_key_bytes());
        bytes.to_vec()
    }
}

impl DHKE for X25519KeyPair {
    fn key_exchange(&self, key: &Self) -> Result<Vec<u8>, Error> {
        match &(self.secret_key) {
            Some(x) => Ok(x.diffie_hellman(&key.public_key).as_bytes().to_vec()),
            None => Err(Error::KeyPairError("secret key not present".to_owned())),
        }
    }
}

impl DSA for X25519KeyPair {
    fn sign(&self, _: Payload) -> Result<Vec<u8>, Error> {
        unimplemented!("DSA is not supported for this key type")
    }

    fn verify(&self, _: Payload, _: &[u8]) -> Result<(), Error> {
        unimplemented!("DSA is not supported for this key type")
    }
}

/// Serde compatible Serialize
impl Serialize for X25519KeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_str())
    }
}

/// Serde compatible Deserialize
impl<'de> Deserialize<'de> for X25519KeyPair {
    fn deserialize<D>(deserializer: D) -> Result<X25519KeyPair, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = decode_config(&s, base64::URL_SAFE).map_err(serde::de::Error::custom)?;

        Ok(X25519KeyPair::from_secret_key(&bytes[..SECRET_KEY_LENGTH]))
    }
}

fn clone_into_array<A, T>(slice: &[T]) -> A
where
    A: Sized + Default + AsMut<[T]>,
    T: Clone,
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
}

#[cfg(test)]
mod tests {

    use super::X25519KeyPair;
    use crate::commons::crypto::{KeyGenerator, DHKE};

    #[test]
    fn test_ser_des() {
        let kp1 = X25519KeyPair::new();
        let kp2 = X25519KeyPair::new();
        let secret1 = kp1.key_exchange(&kp2).unwrap();
        let kp_str = serde_json::to_string_pretty(&kp1).unwrap();
        let new_kp: Result<X25519KeyPair, serde_json::Error> = serde_json::from_str(&kp_str);
        assert!(new_kp.is_ok());
        let secret2 = kp2.key_exchange(&new_kp.unwrap()).unwrap();
        assert_eq!(secret1, secret2);
    }
}
