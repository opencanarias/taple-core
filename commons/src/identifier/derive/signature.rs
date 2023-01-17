//! Self Signing derivation module
//!

use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};

use super::Derivator;
use crate::identifier::{error::Error, signature_identifier::SignatureIdentifier};

/// Enumeration with signature derivator types
#[derive(
    Debug, PartialEq, Clone, Copy, Eq, Hash, BorshSerialize, BorshDeserialize, PartialOrd,
)]
pub enum SignatureDerivator {
    Ed25519Sha512,
    ECDSAsecp256k1,
}

impl SignatureDerivator {
    pub fn derive(&self, sign: &[u8]) -> SignatureIdentifier {
        SignatureIdentifier::new(*self, sign)
    }
}

impl Derivator for SignatureDerivator {
    fn code_len(&self) -> usize {
        match self {
            Self::Ed25519Sha512 | Self::ECDSAsecp256k1 => 2,
        }
    }

    fn derivative_len(&self) -> usize {
        match self {
            Self::Ed25519Sha512 | Self::ECDSAsecp256k1 => 86,
        }
    }

    fn to_str(&self) -> String {
        match self {
            Self::Ed25519Sha512 => "SE",
            Self::ECDSAsecp256k1 => "SS",
        }
        .into()
    }
}

impl FromStr for SignatureDerivator {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &s[..1] {
            "S" => match &s[1..2] {
                "E" => Ok(Self::Ed25519Sha512),
                "S" => Ok(Self::ECDSAsecp256k1),
                _ => Err(Error::DeserializationError),
            },
            _ => Err(Error::DeserializationError),
        }
    }
}
