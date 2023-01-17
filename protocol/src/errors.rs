use governance::error::RequestError;
use ledger::errors::LedgerManagerError;
use std::convert::Infallible;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProtocolErrors {
    #[error("Deserialization error")]
    DeserializationError,
    #[error("Serialization error")]
    SerializationError,
    #[error("Errors that can never happen")]
    InfalibleError {
        #[from]
        source: Infallible,
    },
    #[error("Secret Key not found")]
    SignatureError,
    #[error("Channel unavaible")]
    ChannelError {
        #[from]
        source: commons::errors::ChannelErrors,
    },
    #[error("Ledger State data incorrect")]
    LedgerStateError,
    #[error("Oneshot channel not available")]
    OneshotUnavailable,
    #[error("Ledger response not expected")]
    UnexpectedLedgerResponse,
    #[error("Governance response not expected")]
    UnexpectedGovernanceResponse,
    #[error("Ledger error")]
    LedgerError {
        #[from]
        source: LedgerManagerError,
    },
    #[error("Not validator")]
    NotValidator,
    #[error("Invalid combination for GovernanceID or SchemaID: Neither Governace or subject")]
    NotValidIDs,
    #[error("Governance error")]
    GovernanceError {
        #[from]
        source: RequestError,
    },
    #[error("Can't send notification")]
    NotificationError,
}

#[derive(Error, Debug, Clone)]
pub enum EventCreationError {
    #[error("No owner of subject")]
    NoOwnerOfSubject,
    #[error("Event creation not possible")]
    EventCreationNotAvailable,
    #[error("Event creation failed")]
    EventCreationFailed,
    #[error("Subject not available for new events")]
    SubjectNotAvailable,
}

#[derive(Error, Debug, Clone)]
pub enum ResponseError {
    #[error("Subject not found")]
    SubjectNotFound,
    #[error("Event not found")]
    EventNotFound,
    #[error("EventCreationError")]
    EventCreationError {
        #[from]
        source: EventCreationError,
    },
    #[error("Invalid operation detected in Ledger")]
    InvalidOperation {
        #[from]
        source: LedgerManagerError,
    },
    #[error("Comunnication with Leyer closed")]
    LedgerChannelClosed,
    #[error("Comunnication with manager closed")]
    ComunnicationClosed,
    #[error("Unexpect Command Response")]
    UnexpectedCommandResponse,
    #[error("Not valid set operation")]
    InvalidSetOperation,
    #[error("Simulation failed")]
    SimulationFailed,
    #[error("Approval is not needed")]
    ApprovalNotNeeded,
    #[error("Subject not synchronized")]
    NoSynchronizedSubject,
    #[error("Invalid caller of event request")]
    InvalidCaller,
    #[error("Subject already being approvald")]
    SubjectNotAvailable,
    #[error("SN not expected")]
    UnexpectedSN,
    #[error("Invalid Hash in ApprovalResponse")]
    InvalidHash,
    #[error("Can't process approval. Not controller of subejct being approval")]
    NotOwnerOfSubject,
    #[error("Vote Not Needed")]
    VoteNotNeeded,
    #[error("Request not found")]
    RequestNotFound,
    #[error("Request already Know")]
    RequestAlreadyKnown,
    #[error("Request Type not supported")]
    RequestTypeError,
    #[error("Event request verification against schema failed")]
    EventRequestVerificationFailed,
    #[error("Schema not found")]
    SchemaNotFound,
}

#[derive(Error, Debug, Clone)]
pub enum RequestManagerError {
    #[error("Input Channel closed")]
    ChannelClosed,
    #[error("Channel with command Manager closed")]
    ComunicationWithCommandManagerClosed,
    #[error("Channel with Governance Manager closed")]
    ComunicationWithGovernanceManagerClosed,
    #[error("Unexpected ASK/TELL request")]
    UnexpectedAnswerModel,
    #[error("BORSH deserialization error")]
    BorshDeserializationError,
    #[error("Sign verification failed")]
    SignVerificationFailed,
    #[error("Event request signature was not possible")]
    SignError,
    #[error("Request Error")]
    RequestError(RequestError),
    #[error("Command Manager Error")]
    CommandManagerError {
        #[from]
        source: ResponseError,
    },
    #[error("Database corrupted {0}")]
    DatabaseCorrupted(String),
}
