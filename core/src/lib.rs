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
//! use taple_core::crypto::*;
//! use taple_core::request::*;
//! use taple_core::signature::*;
//! use taple_core::*;
//!
//! /**
//!  * Basic usage of TAPLE Core. It includes:
//!  * - Node inicialization with on-memory DB (only for testing purpouse)
//!  * - Minimal governance creation example
//!  */
//! #[tokio::main]
//! async fn main() {
//!     // Generate ramdom node ID key pair
//!     let node_key_pair = crypto::KeyPair::Ed25519(Ed25519KeyPair::from_seed(&[]));
//!
//!    // Configure minimal settings
//!    let settings = {
//!        let mut settings = Settings::default();
//!        settings.node.secret_key = hex::encode(node_key_pair.secret_key_bytes());
//!        settings
//!    };
//!
//!    // Build node
//!    let (mut node, api) = Node::build(settings, MemoryManager::new()).expect("TAPLE node built");
//!
//!     // Create a minimal governance
//!     // Compose and sign the subject creation request
//!     let governance_key = api
//!         .add_keys(KeyDerivator::Ed25519)
//!         .await
//!         .expect("Error getting server response");
//!     let create_subject_request = EventRequest::Create(StartRequest {
//!         governance_id: DigestIdentifier::default(),
//!         name: "".to_string(),
//!         namespace: "".to_string(),
//!         schema_id: "governance".to_string(),
//!         public_key: governance_key,
//!     });
//!     let signed_request = Signed::<EventRequest> {
//!         content: create_subject_request.clone(),
//!         signature: Signature::new(&create_subject_request, &node_key_pair).unwrap(),
//!     };
//!
//!     // Send the signed request to the node
//!     let _request_id = api.external_request(signed_request).await.unwrap();
//!
//!     // Wait until event notification
//!     let subject_id = if let Notification::NewEvent { sn: _, subject_id } = node
//!         .recv_notification()
//!         .await
//!         .expect("NewEvent notification received")
//!     {
//!         subject_id
//!     } else {
//!         panic!("Unexpected notification");
//!     };
//!
//!     // Get the new subject data
//!     let subject = api
//!         .get_subject(DigestIdentifier::from_str(&subject_id).unwrap())
//!         .await
//!         .unwrap_or_else(|_| panic!("Error getting subject"));
//!
//!     println!("Subject ID: {}", subject.subject_id.to_str());
//!
//!     node.shutdown_gracefully().await;
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

mod node;
pub use api::{Api, ApiError};
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
    errors::ListenAddrErrors,
    identifier::derive::{digest::DigestDerivator, KeyDerivator},
    models::notification::Notification,
    models::timestamp::TimeStamp,
    models::validation::ValidationProof,
    models::value_wrapper::ValueWrapper,
    settings::{ListenAddr, NetworkSettings, NodeSettings, Settings},
};
pub(crate) use database::DB;
pub use database::{
    DatabaseCollection, DatabaseManager, Error as DbError, MemoryCollection, MemoryManager,
};
pub use error::Error;
pub use node::Node;
