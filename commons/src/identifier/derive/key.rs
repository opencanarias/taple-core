//! Key derivation module
//!

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::{Derivator, SignatureDerivator};
use crate::identifier::{error::Error, key_identifier::KeyIdentifier};

/// Enumeration with key derivator types
#[derive(
    Debug,
    PartialEq,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Eq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    PartialOrd,
)]
pub enum KeyDerivator {
    Ed25519,
    Secp256k1,
}

impl KeyDerivator {
    pub fn derive(&self, public_key: &[u8]) -> KeyIdentifier {
        KeyIdentifier::new(*self, public_key)
    }

    pub fn to_signature_derivator(&self) -> SignatureDerivator {
        match self {
            KeyDerivator::Ed25519 => SignatureDerivator::Ed25519Sha512,
            KeyDerivator::Secp256k1 => SignatureDerivator::ECDSAsecp256k1,
        }
    }
}

impl Derivator for KeyDerivator {
    fn code_len(&self) -> usize {
        match self {
            Self::Ed25519 | Self::Secp256k1 => 1,
        }
    }

    fn derivative_len(&self) -> usize {
        match self {
            Self::Ed25519 => 43,
            Self::Secp256k1 => 87,
        }
    }

    fn to_str(&self) -> String {
        match self {
            Self::Ed25519 => "E",
            Self::Secp256k1 => "S",
        }
        .into()
    }
}

impl FromStr for KeyDerivator {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &s[..1] {
            "E" => Ok(Self::Ed25519),
            "S" => Ok(Self::Secp256k1),
            _ => Err(Error::DeserializationError),
        }
    }
}

impl From<KeyDerivator> for config::Value {
    fn from(data: KeyDerivator) -> Self {
        match data {
            KeyDerivator::Ed25519 => {
                Self::new(None, config::ValueKind::String("Ed25519".to_owned()))
            }
            KeyDerivator::Secp256k1 => {
                Self::new(None, config::ValueKind::String("Secp256k1".to_owned()))
            }
        }
    }
}
