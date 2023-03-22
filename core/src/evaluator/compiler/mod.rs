use crate::{commons, identifier::DigestIdentifier};

use super::errors::CompilerErrorResponses;

mod compiler;
mod manager;

#[derive(Clone, Debug)]
pub enum CompilerMessages {
    CompileContracts(CompileContracts),
}

#[derive(Clone, Debug)]
pub enum CompilerResponses {
    CompileContract(Result<(), CompilerErrorResponses>),
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum ContractType {
    String(String),
}

impl ContractType {
    pub fn get_digest(&self) -> Result<DigestIdentifier, commons::errors::Error> {
        match self {
            ContractType::String(data) => DigestIdentifier::from_serializable_borsh(data),
        }
    }

    pub fn to_string(self) -> String {
        match self {
            ContractType::String(string) => string,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompileContracts {
    pub governance_id: DigestIdentifier,
    governance_version: u64,
}
