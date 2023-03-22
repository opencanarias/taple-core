use thiserror::Error;

use crate::governance::error::RequestError;

#[derive(Error, Debug)]
pub enum EvaluatorError {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Can't accept TELL messages over MSPC channel")]
    TellNotAvailable,
    #[error("Channel not available")]
    ChannelNotAvailable
}

#[derive(Error, Debug, Clone)]
pub enum EvaluatorErrorResponses {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String)
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
    JSONPATCHDeserializationFailed
}

#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("A database error has ocurred at main component {0}")]
    DatabaseError(String),
    #[error("Channel not available")]
    ChannelNotAvailable,
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
}
