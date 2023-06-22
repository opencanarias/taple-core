use thiserror::Error;

use crate::governance::error::RequestError;

#[derive(Error, Debug)]
pub enum EvaluatorError {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Channel not available")]
    ChannelNotAvailable,
    #[error("Create Request not allowed")]
    CreateRequestNotAllowed,
    #[error("JSON Deserialization failed")]
    JSONDeserializationFailed,
    #[error("Signature generation failed")]
    SignatureGenerationFailed,    
}

#[derive(Error, Debug, Clone)]
pub enum EvaluatorErrorResponses {
    #[error("Create Request not allowed")]
    CreateRequestNotAllowed,
    #[error("Contract execution error: \"{0}\"")]
    ContractExecutionError(ExecutorErrorResponses)
}

#[derive(Error, Debug, Clone)]
pub enum ExecutorErrorResponses {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Contract for schema {0} of governance {1} not found")]
    ContractNotFound(String, String),
    #[error("The contract could not be instantiated")]
    ContractNotInstantiated,
    #[error("Contract entrypoint not found")]
    ContractEntryPointNotFound,
    #[error("Contract execution failed")]
    ContractExecutionFailed,
    #[error("Function \"{0}\" could not be linked")]
    FunctionLinkingFailed(String),
    #[error("Deserialization of state failed")]
    StateJSONDeserializationFailed,
    #[error("Deserialization of JSON PATCH failed")]
    JSONPATCHDeserializationFailed,
    #[error("State hash generation failed")]
    StateHashGenerationFailed,
    #[error("Context hash generation failed")]
    ContextHashGenerationFailed,
    #[error("Invalid pointer provided by contract")]
    InvalidPointerPovided,
    #[error("Can't get roles of invokator")]
    RolesObtentionFailed,
    #[error("Cant genererate Contract Result")]
    CantGenerateContractResult,
    #[error("Our Gov Version is Lower than sender")]
    OurGovIsLower,
    #[error("Our Gov Version is Higher than sender")]
    OurGovIsHigher,
    #[error("Create Request not allowed")]
    CreateRequestNotAllowed,
    #[error("Governance module error {0}")]
    GovernanceError(#[from] RequestError),
    #[error("Schema compilation failed")]
    SchemaCompilationFailed,
    #[error("Value to string conversion failed")]
    ValueToStringConversionFailed,
    #[error("Borsh serialization failed")]
    BorshSerializationError,
    #[error("Borsh deerialization failed")]
    BorshDeserializationError,
}

#[derive(Error, Debug, Clone)]
pub enum CompilerError {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Channel not available")]
    ChannelNotAvailable,
    #[error("Initialization process error: {0}")]
    InitError(String),
    #[error("Internal Error")]
    InternalError(#[from] CompilerErrorResponses)
}

#[derive(Error, Debug, Clone)]
pub enum CompilerErrorResponses {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("BorshSerialize Contract Error")]
    BorshSerializeContractError,
    #[error("Write File Error")]
    WriteFileError,
    #[error("Cargo Exec Error")]
    CargoExecError,
    #[error("Garbage Collector Error")]
    GarbageCollectorFail,
    #[error("Contract Addition Error")]
    AddContractFail,
    #[error("Governance Error")]
    GovernanceError(#[from] RequestError),
    #[error("Can't create folder at /tmp")]
    TempFolderCreationFailed,
    #[error("Invalid function import found in WAS module")]
    InvalidImportFound,
    #[error("No SDK found")]
    NoSDKFound
}
