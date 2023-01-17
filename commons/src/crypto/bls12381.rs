//! bls12381

use base64::decode_config;
use bbs::prelude::*;
use pairing_plus::{
    bls12_381::{Fr, G1, G2},
    hash_to_field::BaseFromRO,
    serdes::SerDes,
    CurveProjective,
};
use serde::{de::Deserializer, Deserialize, Serialize, Serializer};
use std::convert::TryFrom;

use super::{create_seed, BaseKeyPair, KeyGenerator, KeyMaterial, Payload, DHKE, DSA};
use crate::identifier;
use identifier::error::Error;

pub const SECRET_KEY_LENGTH: usize = 48;

#[derive(Debug, Clone)]
pub struct CyclicGroup {
    pub g1: Vec<u8>,
    pub g2: DeterministicPublicKey,
}

pub type Bls12381KeyPair = BaseKeyPair<CyclicGroup, SecretKey>;

impl KeyGenerator for Bls12381KeyPair {
    fn new() -> Self {
        generate_keypair(None)
    }

    fn from_seed(seed: &[u8]) -> Self {
        // TODO: Analyze whether a modification is necessary based on how the KeyPair is created
        generate_keypair(Some(seed.into()))
    }

    fn from_public_key(public_key: &[u8]) -> Self {
        Self {
            secret_key: None,
            public_key: CyclicGroup {
                g1: public_key[..48].to_vec(),
                g2: DeterministicPublicKey::try_from(public_key[48..].to_vec()).unwrap(),
            },
        }
    }

    fn from_secret_key(private_key: &[u8]) -> Self {
        use sha2::digest::generic_array::{typenum::U48, GenericArray};

        let result: &GenericArray<u8, U48> = GenericArray::<u8, U48>::from_slice(private_key);
        let sk = Fr::from_okm(generic_array::GenericArray::from_slice(result.as_slice()));

        let mut pk1 = G1::one();
        pk1.mul_assign(sk);

        let mut pk1_bytes = Vec::new();
        pk1.serialize(&mut pk1_bytes, true).unwrap();

        let mut pk2 = G2::one();
        pk2.mul_assign(sk);

        let mut pk2_bytes = Vec::new();
        pk2.serialize(&mut pk2_bytes, true).unwrap();

        Self {
            public_key: CyclicGroup {
                g1: pk1_bytes.to_vec(),
                g2: DeterministicPublicKey::try_from(pk2_bytes).unwrap(),
            },
            secret_key: Some(SecretKey::from(sk)),
        }
    }
}

impl KeyMaterial for Bls12381KeyPair {
    fn public_key_bytes(&self) -> Vec<u8> {
        [
            self.public_key.g1.as_slice(),
            self.public_key.g2.to_bytes_compressed_form().as_ref(),
        ]
        .concat()
        .to_vec()
    }

    fn secret_key_bytes(&self) -> Vec<u8> {
        self.secret_key
            .as_ref()
            .map_or(vec![], |x| x.to_bytes_compressed_form().to_vec())
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut all_bytes = self.secret_key_bytes();
        all_bytes.append(&mut self.public_key_bytes());
        all_bytes
    }
}

impl DSA for Bls12381KeyPair {
    fn sign(&self, payload: Payload) -> Result<Vec<u8>, Error> {
        let messages: Vec<SignatureMessage> = match payload {
            Payload::Buffer(_) => {
                return Err(Error::SignError(
                    "Payload type not supported for this key".into(),
                ))
            }
            Payload::BufferArray(m) => m.iter().map(SignatureMessage::hash).collect(),
        };

        let pk = self.public_key.g2.to_public_key(messages.len()).unwrap();
        let signature = match &self.secret_key {
            Some(sk) => Signature::new(&messages, sk, &pk)
                .map_err(|_| Error::SignError("Invalid signature data".into()))?,
            None => return Err(Error::SignError("Secret key not found".to_owned())),
        };
        Ok(signature.to_bytes_compressed_form().to_vec())
    }

    fn verify(&self, payload: Payload, signature: &[u8]) -> Result<(), Error> {
        let messages: Vec<SignatureMessage> = match payload {
            Payload::Buffer(_) => {
                return Err(Error::SignError(
                    "Payload type not supported for this key".into(),
                ))
            }
            Payload::BufferArray(m) => m.iter().map(SignatureMessage::hash).collect(),
        };

        let pk = self.public_key.g2.to_public_key(messages.len()).unwrap();
        let sig = match Signature::try_from(signature) {
            Ok(sig) => sig,
            Err(_) => return Err(Error::SignError("unable to parse signature".into())),
        };

        match sig.verify(&messages, &pk) {
            Ok(x) => {
                if x {
                    Ok(())
                } else {
                    Err(Error::SignError("invalid signature".into()))
                }
            }
            Err(_) => Err(Error::SignError("unexpected error".into())),
        }
    }
}

impl DHKE for Bls12381KeyPair {
    fn key_exchange(&self, _: &Self) -> Result<Vec<u8>, Error> {
        unimplemented!("ECDH is not supported for this key type")
    }
}

// Serde compatible Serialize
impl Serialize for Bls12381KeyPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_str())
    }
}

// Serde compatible Deserialize
impl<'de> Deserialize<'de> for Bls12381KeyPair {
    fn deserialize<D>(deserializer: D) -> Result<Bls12381KeyPair, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = decode_config(&s, base64::URL_SAFE).map_err(serde::de::Error::custom)?;

        Ok(Bls12381KeyPair::from_secret_key(
            &bytes[..SECRET_KEY_LENGTH],
        ))
    }
}

fn generate_keypair(seed: Option<Vec<u8>>) -> Bls12381KeyPair {
    let seed_data = create_seed(seed.map_or(vec![], |x| x).as_slice()).unwrap();

    let sk = gen_sk(seed_data.to_vec().as_slice());
    let mut pk1 = G1::one();
    pk1.mul_assign(sk);

    let mut pk1_bytes = Vec::new();
    pk1.serialize(&mut pk1_bytes, true).unwrap();

    let mut pk2 = G2::one();
    pk2.mul_assign(sk);

    let mut pk2_bytes = Vec::new();
    pk2.serialize(&mut pk2_bytes, true).unwrap();

    Bls12381KeyPair {
        public_key: CyclicGroup {
            g1: pk1_bytes.to_vec(),
            g2: DeterministicPublicKey::try_from(pk2_bytes).unwrap(),
        },
        secret_key: Some(SecretKey::from(sk)),
    }
}

fn gen_sk(msg: &[u8]) -> Fr {
    use sha2::digest::generic_array::{typenum::U48, GenericArray};
    const SALT: &[u8] = b"BLS-SIG-KEYGEN-SALT-";
    // copy of `msg` with appended zero byte
    let mut msg_prime = Vec::<u8>::with_capacity(msg.len() + 1);
    msg_prime.extend_from_slice(msg.as_ref());
    msg_prime.extend_from_slice(&[0]);
    // `result` has enough length to hold the output from HKDF expansion
    let mut result = GenericArray::<u8, U48>::default();
    assert!(hkdf::Hkdf::<sha2::Sha256>::new(Some(SALT), &msg_prime[..])
        .expand(&[0, 48], &mut result)
        .is_ok());
    Fr::from_okm(generic_array::GenericArray::from_slice(result.as_slice()))
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_ser_des() {
        let key_pair = Bls12381KeyPair::new();
        let messages = vec![
            b"secret message 1".to_vec(),
            b"secret message 2".to_vec(),
            b"secret message 3".to_vec(),
            b"secret message 4".to_vec(),
            b"secret message 5".to_vec(),
        ];
        let signature = key_pair
            .sign(Payload::BufferArray(messages.clone()))
            .unwrap();
        let valid = key_pair.verify(Payload::BufferArray(messages.clone()), &signature);

        matches!(valid, Ok(()));

        let kp_str = serde_json::to_string_pretty(&key_pair).unwrap();
        let new_kp: Bls12381KeyPair = serde_json::from_str(&kp_str).unwrap();
        let signature = new_kp.sign(Payload::BufferArray(messages.clone())).unwrap();
        let valid = key_pair.verify(Payload::BufferArray(messages.clone()), &signature);

        matches!(valid, Ok(()));
    }
}
