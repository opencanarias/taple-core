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
//! use std::str::FromStr;
//!
//! use taple_core::crypto;
//! use taple_core::crypto::Ed25519KeyPair;
//! use taple_core::crypto::KeyGenerator;
//! use taple_core::crypto::KeyMaterial;
//! use taple_core::get_default_settings;
//! use taple_core::request::StartRequest;
//! use taple_core::signature::Signature;
//! use taple_core::signature::Signed;
//! use taple_core::ApiModuleInterface;
//! use taple_core::Derivable;
//! use taple_core::DigestIdentifier;
//! use taple_core::EventRequest;
//! use taple_core::KeyDerivator;
//! use taple_core::KeyIdentifier;
//! use taple_core::MemoryManager;
//! use taple_core::Notification;
//! use taple_core::Taple;
//!
//! /**
//!  * Basic usage of TAPLE Core. It includes:
//!  * - Node inicialization with on-memory DB (only for testing purpouse)
//!  * - Minimal governance creation example
//!  */
//! #[tokio::main]
//! async fn main() -> Result<(), ()> {
//!     // Generate ramdom node cryptographic material and ID
//!     let node_key_pair = crypto::KeyPair::Ed25519(Ed25519KeyPair::from_seed(&[]));
//!     let node_identifier = KeyIdentifier::new(
//!         node_key_pair.get_key_derivator(),
//!         &node_key_pair.public_key_bytes(),
//!     );
//!
//!     // Build and start up the TAPLE node
//!     let mut settings = get_default_settings();
//!     settings.node.secret_key = Some(hex::encode(&node_key_pair.secret_key_bytes()));
//!
//!     let mut taple = Taple::new(settings, MemoryManager::new());
//!     let mut notifier = taple.get_notification_handler();
//!     let shutdown_manager = taple.get_shutdown_manager();
//!     let api = taple.get_api();
//!
//!     taple.start().await.expect("TAPLE started");
//!
//!     // Create a minimal governance
//!     // Compose and sign the subject creation request
//!     let new_key = api
//!         .add_keys(KeyDerivator::Ed25519)
//!         .await
//!         .expect("Error getting server response");
//!
//!     let create_subject_request = EventRequest::Create(StartRequest {
//!         governance_id: DigestIdentifier::default(),
//!         name: "".to_string(),
//!         namespace: "".to_string(),
//!         schema_id: "governance".to_string(),
//!         public_key: new_key,
//!     });
//!
//!     let signed_request = Signed::<EventRequest> {
//!         content: create_subject_request.clone(),
//!         signature: Signature::new(&create_subject_request, node_identifier, &node_key_pair)
//!             .unwrap(),
//!     };
//!
//!     // Send the signed request to the node
//!     let _request_id = api.external_request(signed_request).await.unwrap();
//!
//!     // Wait until notification of subject creation
//!     let subject_id =
//!         if let Notification::NewSubject { subject_id } = notifier.receive().await.unwrap() {
//!             subject_id
//!         } else {
//!             panic!("Unexpected notification");
//!         };
//!
//!     // Get the new subject data
//!     let subject_id =
//!         DigestIdentifier::from_str(&subject_id).expect("DigestIdentifier from str failed");
//!
//!     let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
//!         "Error getting subject content with id: {}",
//!         subject_id.to_str()
//!     ));
//!
//!     println!("{:#?}", subject);
//!
//!     // Shutdown the TAPLE node
//!     shutdown_manager.shutdown().await;
//!
//!     Ok(())
//! }
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
pub mod message;
pub(crate) mod network;
pub(crate) mod utils;
pub(crate) mod validation;

pub(crate) mod event;
pub(crate) mod protocol;

mod unitary_component;
pub use api::{ApiError, ApiModuleInterface, NodeAPI};
// pub(crate) use api::APICommands;
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
    config::{ListenAddr, NodeSettings, TapleSettings},
    errors::ListenAddrErrors,
    identifier::derive::{digest::DigestDerivator, KeyDerivator},
    models::notification::Notification,
    models::timestamp::TimeStamp,
    models::validation::ValidationProof,
    models::value_wrapper::ValueWrapper,
};
pub(crate) use database::DB;
pub use database::{
    DatabaseCollection, DatabaseError as DbError, DatabaseManager, MemoryCollection, MemoryManager,
};
pub use error::Error;
pub use network::TapleNetwork;
pub use unitary_component::{
    get_default_settings, NotificationHandler, Taple, TapleShutdownManager,
};
