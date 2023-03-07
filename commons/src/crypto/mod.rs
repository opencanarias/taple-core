//! This module provides the structs and traits for the generation of key pairs for
//! cryptographic operations.
//!

pub(crate) mod ed25519;
pub(crate) mod error;
#[cfg(feature = "secp256k1")]
pub(crate) mod secp256k1;
#[cfg(feature = "x25519")]
pub(crate) mod x25519;

use identifier::error::Error;

use base64::encode_config;
pub use ed25519::Ed25519KeyPair;
#[cfg(feature = "secp256k1")]
pub use secp256k1::Secp256k1KeyPair;
use serde::{Deserialize, Serialize};
#[cfg(feature = "x25519")]
pub use x25519::X25519KeyPair;

use crate::identifier::{self, derive::KeyDerivator};

/// Asymmetric key pair
#[derive(Serialize, Deserialize, Debug)]
pub enum KeyPair {
    Ed25519(Ed25519KeyPair),
    #[cfg(feature = "secp256k1")]
    Secp256k1(Secp256k1KeyPair),
}

impl KeyPair {
    pub fn get_key_derivator(&self) -> KeyDerivator {
        match self {
            KeyPair::Ed25519(_) => KeyDerivator::Ed25519,
            KeyPair::Secp256k1(_) => KeyDerivator::Secp256k1,
        }
    }
}

// Generate key pair
pub fn generate<T: KeyGenerator + DSA + Into<KeyPair>>(seed: Option<&[u8]>) -> KeyPair {
    T::from_seed(seed.map_or(vec![].as_slice(), |x| x)).into()
}

/// Base for asymmetric key pair
#[derive(Default, Debug, Clone, PartialEq)]
pub struct BaseKeyPair<P, K> {
    pub public_key: P,
    pub secret_key: Option<K>,
}

/// Represents asymetric key pair for storage (deprecated: KeyPair is serializable)
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryptoBox {
    pub public_key: Vec<u8>,
    pub secret_key: Vec<u8>,
}

/// Return key material bytes
pub trait KeyMaterial {
    /// Returns the public key bytes as slice
    fn public_key_bytes(&self) -> Vec<u8>;

    /// Returns the secret key bytes as slice
    fn secret_key_bytes(&self) -> Vec<u8>;

    /// Returns bytes from key pair
    fn to_bytes(&self) -> Vec<u8>;

    /// Returns String from key pair encoded in base64
    fn to_str(&self) -> String {
        encode_config(self.to_bytes(), base64::URL_SAFE_NO_PAD)
    }
}

/// Collection of methods to initialize a key pair
/// using random or deterministic manner
pub trait KeyGenerator: KeyMaterial {
    /// Generates random keys
    fn new() -> Self
    where
        Self: Sized,
    {
        Self::from_seed(vec![].as_slice())
    }

    /// Generates keys deterministically using a given seed
    fn from_seed(seed: &[u8]) -> Self
    where
        Self: Sized;

    /// Generates keys from existing public key
    fn from_public_key(public_key: &[u8]) -> Self
    where
        Self: Sized;

    /// Generate keys from existing secret key
    fn from_secret_key(private_key: &[u8]) -> Self
    where
        Self: Sized;
}

/// Used for Digital Signature Algorithm (DSA)
pub trait DSA {
    /// Performs sign operation
    fn sign(&self, payload: Payload) -> Result<Vec<u8>, Error>;

    /// Performs verify operation
    fn verify(&self, payload: Payload, signature: &[u8]) -> Result<(), Error>;
}

/// Used for Diffie–Hellman key exchange operations
pub trait DHKE {
    /// Perform key exchange operation
    fn key_exchange(&self, their_public: &Self) -> Result<Vec<u8>, Error>;
}

/// Clone key pair
impl Clone for KeyPair {
    fn clone(&self) -> Self {
        match self {
            KeyPair::Ed25519(kp) => {
                KeyPair::Ed25519(Ed25519KeyPair::from_secret_key(&kp.secret_key_bytes()))
            }
            KeyPair::Secp256k1(kp) => {
                KeyPair::Secp256k1(Secp256k1KeyPair::from_secret_key(&kp.secret_key_bytes()))
            }
            // KeyPair::X25519(kp) => KeyPair::X25519(
            //     X25519KeyPair::from_secret_key(&kp.secret_key_bytes()),
            // ),
        }
    }
}

impl KeyMaterial for KeyPair {
    fn public_key_bytes(&self) -> Vec<u8> {
        match self {
            KeyPair::Ed25519(x) => x.public_key_bytes(),
            #[cfg(feature = "secp256k1")]
            KeyPair::Secp256k1(x) => x.public_key_bytes(),
            // #[cfg(feature = "x25519")]
            // KeyPair::X25519(x) => x.public_key_bytes(),
        }
    }

    fn secret_key_bytes(&self) -> Vec<u8> {
        match self {
            KeyPair::Ed25519(x) => x.secret_key_bytes(),
            #[cfg(feature = "secp256k1")]
            KeyPair::Secp256k1(x) => x.secret_key_bytes(),
            // #[cfg(feature = "x25519")]
            // KeyPair::X25519(x) => x.secret_key_bytes(),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        match self {
            KeyPair::Ed25519(x) => x.to_bytes(),
            #[cfg(feature = "secp256k1")]
            KeyPair::Secp256k1(x) => x.to_bytes(),
            // #[cfg(feature = "x25519")]
            // KeyPair::X25519(x) => x.to_bytes(),
        }
    }
}

impl DSA for KeyPair {
    fn sign(&self, payload: Payload) -> Result<Vec<u8>, Error> {
        match self {
            KeyPair::Ed25519(x) => x.sign(payload),
            #[cfg(feature = "secp256k1")]
            KeyPair::Secp256k1(x) => x.sign(payload),
            // _ => Err(Error::KeyPairError(
            //     "DSA is not supported for this key type".to_owned(),
            // )),
        }
    }

    fn verify(&self, payload: Payload, signature: &[u8]) -> Result<(), Error> {
        match self {
            KeyPair::Ed25519(x) => x.verify(payload, signature),
            #[cfg(feature = "secp256k1")]
            KeyPair::Secp256k1(x) => x.verify(payload, signature),
            // #[cfg(feature = "x25519")]
            // KeyPair::X25519(_) => Err(Error::KeyPairError(
            //     "DSA is not supported for this key type".to_owned(),
            // )),
        }
    }
}

// impl DHKE for KeyPair {
//     fn key_exchange(&self, key: &Self) -> Result<Vec<u8>, Error> {
//         match (self, key) {
//             #[cfg(feature = "x25519")]
//             (KeyPair::X25519(me), KeyPair::X25519(them)) => {
//                 me.key_exchange(them)
//             }
//             _ => Err(Error::KeyPairError(
//                 "DHKE is not supported for this key type".to_owned(),
//             )),
//         }
//     }
// }

/// Payloads
#[derive(Debug, Clone)]
pub enum Payload {
    Buffer(Vec<u8>),
    BufferArray(Vec<Vec<u8>>),
}

/// Creates 32 bytes seed
pub fn create_seed(initial_seed: &[u8]) -> Result<[u8; 32], Error> {
    let mut seed = [0u8; 32];
    if initial_seed.is_empty() {
        getrandom::getrandom(&mut seed)
            .map_err(|_| Error::SeedError("couldn't generate random seed".to_owned()))?;
    } else if initial_seed.len() <= 32 {
        seed[..initial_seed.len()].copy_from_slice(initial_seed);
    } else {
        return Err(Error::SeedError("seed is greater than 32".to_owned()));
    }
    Ok(seed)
}

#[cfg(test)]
mod tests {

    use super::{create_seed, ed25519::Ed25519KeyPair, generate, Payload, DSA};

    #[cfg(feature = "secp256k1")]
    use super::secp256k1::Secp256k1KeyPair;

    #[test]
    fn test_create_seed() {
        assert!(create_seed(vec![].as_slice()).is_ok());
        let seed = "48s8j34fuadfeuijakqp93d56829ki21".as_bytes();
        assert!(create_seed(seed).is_ok());
        let seed = "witness".as_bytes();
        assert!(create_seed(seed).is_ok());
        let seed = "witnesssdfasfasfasfsafsafasfsafsafasfasfasdf".as_bytes();
        assert!(create_seed(seed).is_err());
    }

    #[test]
    fn test_ed25519() {
        let key_pair = generate::<Ed25519KeyPair>(None);
        let message = b"secret message";
        let signature = key_pair.sign(Payload::Buffer(message.to_vec())).unwrap();
        println!("Tamaño: {}", signature.len());
        let valid = key_pair.verify(Payload::Buffer(message.to_vec()), &signature);

        matches!(valid, Ok(()));
    }

    #[test]
    #[cfg(feature = "secp256k1")]
    fn test_secp256k1() {
        let key_pair = generate::<Secp256k1KeyPair>(None);
        let message = b"secret message";
        let signature = key_pair.sign(Payload::Buffer(message.to_vec())).unwrap();
        println!("Tamaño: {}", signature.len());
        let valid = key_pair.verify(Payload::Buffer(message.to_vec()), &signature);

        matches!(valid, Ok(()));
    }

    // #[test]
    // #[cfg(feature = "bls12381")]
    // fn test_bls12381() {
    //     let key_pair = generate::<Bls12381KeyPair>(None);
    //     let messages = vec![
    //         b"secret message 1".to_vec(),
    //         b"secret message 2".to_vec(),
    //         b"secret message 3".to_vec(),
    //         b"secret message 4".to_vec(),
    //         b"secret message 5".to_vec(),
    //     ];
    //     let signature = key_pair
    //         .sign(Payload::BufferArray(messages.clone()))
    //         .unwrap();
    //     let valid =
    //         key_pair.verify(Payload::BufferArray(messages.clone()), &signature);

    //     matches!(valid, Ok(()));
    // }

    // #[test]
    // #[cfg(feature = "x25519")]
    // fn test_x25519() {
    //     let key_pair1 = generate::<X25519KeyPair>(None);
    //     let key_pair2 = generate::<X25519KeyPair>(None);
    //     let secret1 = key_pair1.key_exchange(&key_pair2).unwrap();
    //     let secret2 = key_pair2.key_exchange(&key_pair1).unwrap();
    //     assert_eq!(secret1, secret2);
    // }
}
