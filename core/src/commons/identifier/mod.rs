//! Identifiers module
//!

pub mod derive;
pub(crate) mod digest_identifier;
pub(crate) mod error;
pub(crate) mod key_identifier;
pub(crate) mod signature_identifier;

pub use digest_identifier::DigestIdentifier;
pub use key_identifier::KeyIdentifier;
pub use signature_identifier::SignatureIdentifier;

use base64::encode_config;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;

use self::error::Error;

/// Enumeration of Identifier types
#[derive(PartialEq, Debug, Clone, Eq, Hash)]
pub(crate) enum Identifier {
    Key(KeyIdentifier),
    Digest(DigestIdentifier),
    Sign(SignatureIdentifier),
}

impl Identifier {
    /*pub fn payload(&self) -> Vec<u8> {
        match self {
            Identifier::Key(value) => value.public_key.clone(),
            Identifier::Digest(value) => value.digest.clone(),
            Identifier::Sign(value) => value.signature.clone(),
        }
    }*/
}

/// Default implementation for Identifiers
impl Default for Identifier {
    fn default() -> Self {
        Self::Digest(DigestIdentifier::default())
    }
}

/// Derivable Identifiers
pub trait Derivable: FromStr<Err = Error> {
    fn derivative(&self) -> Vec<u8>;

    fn derivation_code(&self) -> String;

    fn to_str(&self) -> String {
        match self.derivative().len() {
            0 => "".to_string(),
            _ => [
                self.derivation_code(),
                encode_config(self.derivative(), base64::URL_SAFE_NO_PAD),
            ]
            .join(""),
        }
    }
}

impl FromStr for Identifier {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(id) = DigestIdentifier::from_str(s) {
            Ok(Identifier::Digest(id))
        } else if let Ok(id) = KeyIdentifier::from_str(s) {
            Ok(Identifier::Key(id))
        } else if let Ok(id) = SignatureIdentifier::from_str(s) {
            Ok(Identifier::Sign(id))
        } else {
            Err(Error::SemanticError(format!("Incorrect Identifier: {}", s)))
        }
    }
}

impl Derivable for Identifier {
    fn derivative(&self) -> Vec<u8> {
        match self {
            Identifier::Key(d) => d.derivative(),
            Identifier::Digest(d) => d.derivative(),
            Identifier::Sign(d) => d.derivative(),
        }
    }

    fn derivation_code(&self) -> String {
        match self {
            Identifier::Key(d) => d.derivation_code(),
            Identifier::Digest(d) => d.derivation_code(),
            Identifier::Sign(d) => d.derivation_code(),
        }
    }
}

impl Serialize for Identifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_str())
    }
}

impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D>(deserializer: D) -> Result<Identifier, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Identifier::from_str(&s).map_err(serde::de::Error::custom)
    }
}
