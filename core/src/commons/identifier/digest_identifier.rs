use crate::identifier::derive::{digest::DigestDerivator, Derivator};
use base64::decode_config;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use super::error::Error;
use super::Derivable;

/// Digest based identifier
#[derive(Debug, PartialEq, Clone, Eq, Hash, BorshSerialize, BorshDeserialize, PartialOrd)]
pub struct DigestIdentifier {
    pub derivator: DigestDerivator,
    pub digest: Vec<u8>,
}

impl DigestIdentifier {
    pub fn from_serializable<S: Serialize>(data: &S) -> Result<Self, Error> {
        let bytes = serde_json::to_vec(data)?;
        let bytes = DigestDerivator::Blake3_256.digest(&bytes);
        Ok(DigestIdentifier::new(DigestDerivator::Blake3_256, &bytes))
    }

    pub fn from_serializable_borsh<T: BorshSerialize>(
        serializable: T,
    ) -> Result<Self, crate::commons::errors::Error> {
        let bytes = match serializable.try_to_vec() {
            Ok(bytes) => bytes,
            Err(_) => return Err(crate::commons::errors::Error::BorshSerializationFailed),
        };
        let bytes = DigestDerivator::Blake3_256.digest(&bytes);
        Ok(DigestIdentifier::new(DigestDerivator::Blake3_256, &bytes))
    }

    pub fn new(derivator: DigestDerivator, digest: &[u8]) -> Self {
        Self {
            derivator,
            digest: digest.to_vec(),
        }
    }
}

impl Default for DigestIdentifier {
    fn default() -> Self {
        Self {
            derivator: DigestDerivator::Blake3_256,
            digest: vec![],
        }
    }
}

impl Display for DigestIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_str(),)
    }
}

/// From string to KeyIdentifier
impl FromStr for DigestIdentifier {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(DigestIdentifier::default());
        }
        let code = DigestDerivator::from_str(s)?;
        if s.len() == code.material_len() {
            Ok(Self::new(
                code,
                &decode_config(&s[code.code_len()..code.material_len()], base64::URL_SAFE)?,
            ))
        } else {
            Err(Error::SemanticError(format!(
                "Incorrect Prefix Length: {}",
                s
            )))
        }
    }
}

impl Derivable for DigestIdentifier {
    fn derivative(&self) -> Vec<u8> {
        self.digest.to_owned()
    }
    fn derivation_code(&self) -> String {
        self.derivator.to_str()
    }
}

impl Serialize for DigestIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_str())
    }
}

impl<'de> Deserialize<'de> for DigestIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<DigestIdentifier, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <std::string::String as Deserialize>::deserialize(deserializer)?;
        if s.is_empty() {
            Ok(DigestIdentifier::default())
        } else {
            DigestIdentifier::from_str(&s).map_err(serde::de::Error::custom)
        }
    }
}

#[cfg(test)]
mod tests {

    use super::DigestIdentifier;

    #[test]
    fn test_digest_identifier_serde() {
        let id = DigestIdentifier::default();
        let id_str = serde_json::to_string_pretty(&id).unwrap();
        let new_id: DigestIdentifier = serde_json::from_str(&id_str).unwrap();
        assert_eq!(id, new_id);
    }
}
