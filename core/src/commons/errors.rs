use ed25519_dalek::ed25519;
use std::convert::Infallible;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Schema Creation Error")]
    SchemaCreationError,
    #[error("Errors that can never happen")]
    InfalibleError {
        #[from]
        source: Infallible,
    },

    #[error("Unknown error: `{0}`")]
    UnknownError(String),

    #[error("Verification error: {0}")]
    VerificationError(String),

    #[error("`{0}`")]
    PayloadError(String),

    #[error("Base58 Decoding error")]
    Base64DecodingError {
        #[from]
        source: base64::DecodeError,
    },

    #[error("Ed25519 error")]
    Ed25519Error {
        #[from]
        source: ed25519::Error,
    },

    #[error("Serde JSON error")]
    SerdeJson {
        #[from]
        source: serde_json::Error,
    },

    #[error("Serde CBOR error")]
    SerdeCbor {
        #[from]
        source: serde_cbor::Error,
    },

    #[error("MessagePack serialize error")]
    MsgPackSerialize {
        #[from]
        source: rmp_serde::encode::Error,
    },

    #[error("MessagePack deserialize error")]
    MsgPackDeserialize {
        #[from]
        source: rmp_serde::decode::Error,
    },

    #[error("Event error: {0}")]
    EventError(String),

    #[error("Seed error: {0}")]
    SeedError(String),

    #[error("Semantic error: {0}")]
    SemanticError(String),

    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),

    #[error("Sign error: {0}")]
    SignError(String),

    #[error("No signature error: {0}")]
    NoSignatureError(String),

    #[error("Key pair error: {0}")]
    KeyPairError(String),

    #[error("TAPLE error: {0}")]
    TapleError(String),

    #[error("Store error: {0}")]
    StoreError(String),

    #[error("Duplicate Event")]
    DuplicateEvent,

    #[error("Event out of order")]
    OutOfOrder,

    #[error("Schema not found")]
    SchemaNotFoundError,

    #[error("Subject not found")]
    SubjectNotFoundError,

    #[error("Memory Database write fails")]
    MemoryDBWriteFailed,

    #[error("Serialization with Borsh fails")]
    BorshSerializationFailed,
}

#[derive(Error, Debug)]
pub enum ChannelErrors {
    #[error("Channel is closed at the other end. Cannot send data")]
    ChannelClosed,
    #[error("Consumer queue is full.")]
    FullQueue,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum CryptoErrorEvent {
    #[error("Request Signature is not valid")]
    RequestSignatureInvalid,
    #[error("Subject Signature is not valid")]
    SubjectSignatureInvalid,
    #[error("Error Hashing event content")]
    EventContentHashingError,
    #[error("Error Hashing event request")]
    EventRequestHashingError,
    #[error("Event content hash is not equal to signature hash")]
    EventContentHashingConflict,
    #[error("Event request hash is not equal to signature hash")]
    EventRequestHashingConflict,
}

#[derive(Error, Debug, PartialEq, Clone)]
pub enum SubjectError {
    #[error("Event request type is not Create")]
    NotCreateEvent,
    #[error("Event request type is not State")]
    NotStateEvent,
    #[error("An event is already created waiting to get quorum")]
    EventAlreadyProcessing,
    #[error("An event which is already applied is not in the database")]
    EventAlreadyAppliedNotFound,
    #[error("Event SN is not 0")]
    SnNot0,
    #[error("Event sourcing is not in order")]
    EventSourcingNotInOrder(u64, u64),
    #[error("Hash of Subject_data after apply does not match to event subject_data_hash")]
    EventSourcingHashNotEqual,
    #[error("Applying to Subject without data")]
    ApplyInEmptySubject,
    #[error("Subject Not Found")]
    SubjectNotFound,
    #[error("We are not the owner of the subject")]
    NotOwnerOfSubject,
    #[error("Event Content failed at serialization")]
    EventContentSerializationFailed,
    #[error("Subject Signature Failed")]
    SubjectSignatureFailed,
    #[error("Subject has no data")]
    SubjectHasNoData,
    #[error("Delete Signatures Failed")]
    DeleteSignaturesFailed,
    #[error("Schema Validation Failed")]
    SchemaValidationFailed,
    #[error("Schema does not compile")]
    SchemaDoesNotCompile,
    #[error("Error in criptography")]
    CryptoError(CryptoErrorEvent),
    #[error("InvalidPayload {0}")]
    InvalidPayload(String),
    #[error("Error parsing json string")]
    ErrorParsingJsonString,
    #[error("Error applying patch")]
    ErrorApplyingPatch,
    #[error("Duplicated schema or member")]
    DuplicatedSchemaOrMember,
    #[error("Policies Missing for Some Schema")]
    PoliciesMissing,
    #[error("Invalid Policies Id")]
    InvalidPoliciesId,
    #[error("Invalid Member in Policies")]
    InvalidMemberInPolicies,
    #[error("Invalid member identifier {0}")]
    InvalidMemberIdentifier(String),
    #[error("JSON-PATCH on Create Event not allowed")]
    InvalidUseOfJSONPATCH,
    #[error("Approvers is not subset of validators")]
    ApproversAreNotValidators,
}
