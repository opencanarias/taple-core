#![recursion_limit = "256"]
//! TAPLE is a DLT focused on traceability characterized by its level of scalability,
//! its flexibility to be employed in different devices and use cases and its reduced resource consumption,
//! including power consumption.
//!
//! The TAPLE crate provides the library that allows instantiating nodes of this DLT in order to create a
//! functional network through a single structure containing all the required logic.
//! Applications can interact with these nodes through the API they expose, thus enabling read and write operations
//! against the network. The API also allows the design and creation of customized clients for the technology
//! according to the user's needs.
//!
//! In addition to the node itself, the library also exposes a series of data structures specific to the protocol
//! that can be obtained when interacting with the API or, in some cases, may be necessary to interact with it.
//!
//! # Basic usage
//! ```
//! use crate::crypto::Ed25519KeyPair;
//! use crate::crypto::KeyGenerator;
//! use crate::crypto::KeyMaterial;
//! use crate::database::MemoryCollection;
//! use crate::request::StartRequest;
//! use crate::signature::Signature;
//! use crate::signature::Signed;
//! use instant::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), ()> {
//!    let mut settings = Taple::<MemoryManager, MemoryCollection>::get_default_settings();
//!   // Generate ramdon cryptographic material
//!    let keypair = Ed25519KeyPair::from_seed(&[0; 32]);
//!    let hex_private_key = hex::encode(&keypair.secret_key_bytes());
//!    settings.node.secret_key = Some(hex_private_key);
//!    let kp = crypto::KeyPair::Ed25519(keypair);
//!    let public_key = kp.public_key_bytes();
//!    let key_identifier = KeyIdentifier::new(kp.get_key_derivator(), &public_key);
//!
//!    let mut taple = Taple::new(settings, MemoryManager::new());
//!    // The TAPLE node generates several Tokyo tasks to manage the different
//!    // components of its architecture.
//!    // The "start" method initiates these tasks and returns the control flow.
//!    taple.start().await.expect("TAPLE started");
//!    // From this point the user can start interacting with the node.
//!    // It is the user's responsibility to decide whether to keep the node running.
//!    // To do so, the main thread of the application must not terminate.
//!    let api = taple.get_api();
//!
//!    // First we need to add the keys for our Subject
//!    let pk_added = api
//!        .add_keys(KeyDerivator::Ed25519)
//!        .await
//!        .expect("Error getting server response");
//!
//!    // Create the request
//!    let event_request = EventRequest::Create(StartRequest {
//!        governance_id: DigestIdentifier::default(),
//!        name: "Paco23".to_string(),
//!        namespace: "".to_string(),
//!        schema_id: "governance".to_string(),
//!        public_key: pk_added,
//!    });
//!
//!    let signature = Signature::new(&event_request, key_identifier, &kp).unwrap();
//!
//!    let er_signed = Signed::<EventRequest> {
//!        content: event_request.clone(),
//!        signature,
//!    };
//!
//!    // First we need to create the governance, the game set of rules of our future network, to start creating subject on it.
//!    let request_id = api.external_request(er_signed).await.unwrap();
//!    tokio::time::sleep(Duration::from_millis(1000)).await;
//!
//!    // Next we will send the request to create a governance and we will save the response in a variable for later use.
//!    let subject_id = api
//!        .get_request(request_id)
//!        .await
//!        .unwrap()
//!        .subject_id
//!        .unwrap();
//!
//!    // wait until validation phase is resolved
//!    let max_attemps = 4;
//!    let mut attemp = 0;
//!    while attemp <= max_attemps {
//!        if let Ok(data) = api.get_validation_proof(subject_id.clone()).await {
//!            if data.0.len() == 1 {
//!                break;
//!            }
//!        }
//!        tokio::time::sleep(Duration::from_millis(500)).await;
//!        attemp += 1;
//!    }
//!    // Our governance is treated like a subject so, when we create it, inside the response, we have it's subject_id.
//!    // We can use this to retrieve our governance data:
//!    let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
//!        "Error getting subject content with id: {}",
//!        subject_id.to_str()
//!    ));
//!
//!    println!("Governance subject Id: {:#?}", subject.subject_id.to_str());
//!    println!("Governance subject SN: {:#?}", subject.sn);
//!
//!    // Now we send a signal to stop our TAPLE node:
//!    api.shutdown().await.expect("TAPLE shutdown");
//!    Ok(())
//!}
//! ```
//!
pub(crate) mod api;
pub(crate) mod approval;
pub(crate) mod authorized_subjecs;
pub(crate) mod commons;
pub(crate) mod database;
pub(crate) mod distribution;
pub mod error;
pub(crate) mod evaluator;
pub(crate) mod governance;
pub(crate) mod ledger;
pub(crate) mod message;
pub(crate) mod network;
pub(crate) mod utils;
pub(crate) mod validation;

pub mod event;
pub mod protocol;

mod unitary_component;
pub use api::{
    APICommands, ApiError, ApiModuleInterface, ApiResponses, GetEvents, GetSubject, GetSubjects,
    NodeAPI,
};
pub use commons::crypto;
pub use commons::identifier;
pub use commons::identifier::{Derivable, DigestIdentifier, KeyIdentifier, SignatureIdentifier};
pub use commons::models::approval::ApprovalRequest;
pub use commons::models::approval::ApprovalResponse;
pub use commons::models::approval::{ApprovalEntity, ApprovalState};
pub use commons::models::evaluation::EvaluationRequest;
pub use commons::models::evaluation::EvaluationResponse;
pub use commons::models::event::Event;
pub use commons::models::event::Metadata;
pub use commons::models::request;
pub use commons::models::request::EventRequest;
pub use commons::models::signature;
pub use commons::models::state::SubjectData;
pub use commons::{
    config::{ListenAddr, NetworkSettings, NodeSettings, TapleSettings},
    errors::ListenAddrErrors,
    identifier::derive::{digest::DigestDerivator, KeyDerivator},
    models::notification::Notification,
    models::timestamp::TimeStamp,
    models::validation::ValidationProof,
    models::value_wrapper::ValueWrapper,
};
pub(crate) use database::DB;
pub use database::{DatabaseCollection, DatabaseManager, Error as DbError, MemoryManager};
pub use error::Error;
pub use unitary_component::{
    get_default_settings, NotificationHandler, Taple, TapleShutdownManager,
};
