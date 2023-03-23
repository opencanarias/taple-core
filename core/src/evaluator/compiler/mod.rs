use crate::{commons, identifier::DigestIdentifier};

use super::errors::CompilerErrorResponses;
use base64::decode;
mod compiler;
mod manager;

#[derive(Clone, Debug)]
pub enum CompilerMessages {
    NewGovVersion(NewGovVersion),
}

#[derive(Clone, Debug)]
pub enum CompilerResponses {
    CompileContract(Result<(), CompilerErrorResponses>),
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum ContractType {
    String(String),
    Base64(String),
}

impl ContractType {
    pub fn get_digest(&self) -> Result<DigestIdentifier, commons::errors::Error> {
        match self {
            ContractType::String(data) => DigestIdentifier::from_serializable_borsh(data),
            ContractType::Base64(data) => {
                let decoded_bytes = decode(data)
                    .map_err(|e| commons::errors::Error::Base64DecodingError { source: e })?;

                DigestIdentifier::from_serializable_borsh(
                    &String::from_utf8(decoded_bytes).map_err(|_| {
                        commons::errors::Error::VerificationError(
                            "La cadena decodificada no es UTF-8 válida".into(),
                        )
                    })?,
                )
            }
        }
    }

    pub fn to_string(self) -> Result<String, commons::errors::Error> {
        match self {
            ContractType::String(string) => Ok(string),
            ContractType::Base64(data) => {
                let decoded_bytes = decode(data)
                    .map_err(|e| commons::errors::Error::Base64DecodingError { source: e })?;

                String::from_utf8(decoded_bytes).map_err(|_| {
                    commons::errors::Error::VerificationError(
                        "La cadena decodificada no es UTF-8 válida".into(),
                    )
                })
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewGovVersion {
    pub governance_id: DigestIdentifier,
    governance_version: u64,
}
