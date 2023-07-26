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
//!use std::str::FromStr;
//!
//!use taple_core::Notification;
//!use taple_core::crypto;
//!use taple_core::crypto::Ed25519KeyPair;
//!use taple_core::crypto::KeyGenerator;
//!use taple_core::crypto::KeyMaterial;
//!use taple_core::get_default_settings;
//!use taple_core::request::StartRequest;
//!use taple_core::signature::Signature;
//!use taple_core::signature::Signed;
//!use taple_core::ApiModuleInterface;
//!use taple_core::Derivable;
//!use taple_core::DigestIdentifier;
//!use taple_core::EventRequest;
//!use taple_core::KeyDerivator;
//!use taple_core::KeyIdentifier;
//!use taple_core::MemoryManager;
//!use taple_core::Taple;
//!
//!#[tokio::main]
//!async fn main() -> Result<(), ()> {
//!    let tmp_path = std::env::temp_dir();
//!    let path = format!("{}/.taple/contracts", tmp_path.display());
//!    std::fs::create_dir_all(&path).expect("Contract folder not created");
//!
//!    let mut settings = get_default_settings();
//!
//!    // Generate ramdon cryptographic material
//!    let keypair = Ed25519KeyPair::from_seed(&[0; 32]);
//!    let hex_private_key = hex::encode(&keypair.secret_key_bytes());
//!    settings.node.secret_key = Some(hex_private_key);
//!    let kp = crypto::KeyPair::Ed25519(keypair);
//!    let public_key = kp.public_key_bytes();
//!    let key_identifier = KeyIdentifier::new(kp.get_key_derivator(), &public_key);
//!
//!    // We build the TAPLE node
//!    let mut taple = Taple::new(settings, MemoryManager::new());
//!    // We obtain the notification channel
//!    let mut notifier = taple.get_notification_handler();
//!    // We obtain the shutdown manager
//!    let shutdown_manager = taple.get_shutdown_manager();
//!    // We initialize the node
//!    taple.start().await.expect("TAPLE started");
//!    let api = taple.get_api();
//!
//!    // The minimal step in TAPLE is to create a governance
//!
//!    // First we need to add the keys for our governance, which internally it is a subject
//!    let pk_added = api
//!        .add_keys(KeyDerivator::Ed25519)
//!        .await
//!        .expect("Error getting server response");
//!
//!    // Create the request
//!    let event_request = EventRequest::Create(StartRequest {
//!        governance_id: DigestIdentifier::default(),
//!        name: "".to_string(),
//!        namespace: "".to_string(),
//!        schema_id: "governance".to_string(),
//!        public_key: pk_added,
//!    });
//!
//!    // We sign the request
//!    let signature = Signature::new(&event_request, key_identifier, &kp).unwrap();
//!    let er_signed = Signed::<EventRequest> {
//!        content: event_request.clone(),
//!        signature,
//!    };
//!
//!    // We send the request to the node
//!    let _request_id = api.external_request(er_signed).await.unwrap();
//!
//!    // We wait until the notification of a new subject created is received
//!
//!    let subject_id = match notifier.receive().await {
//!        Ok(data) => {
//!            if let Notification::NewSubject { subject_id } = data {
//!                subject_id
//!            } else {
//!                panic!("Unexpected notification");
//!            }
//!        },
//!        Err(_) => {
//!            panic!("Notifier error")
//!        }
//!    };
//!
//!    // Next, we convert the subject_id to a DigestIdentifier
//!    let subject_id = DigestIdentifier::from_str(&subject_id).expect("DigestIdentifier from str failed");
//!
//!    // We use this ID to retrieve governance data
//!    let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
//!        "Error getting subject content with id: {}",
//!        subject_id.to_str()
//!    ));
//!
//!    // We print the governance received
//!    println!("{:#?}", subject);
//!
//!    // Now we send a signal to stop our TAPLE node:
//!    shutdown_manager.shutdown().await;
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

pub(crate) mod event;
pub(crate) mod protocol;

mod unitary_component;
pub use api::{
    ApiError, ApiModuleInterface,
    NodeAPI,
};
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
    config::{ListenAddr, NetworkSettings, NodeSettings, TapleSettings},
    errors::ListenAddrErrors,
    identifier::derive::{digest::DigestDerivator, KeyDerivator},
    models::notification::Notification,
    models::timestamp::TimeStamp,
    models::validation::ValidationProof,
    models::value_wrapper::ValueWrapper,
};
pub(crate) use database::DB;
pub use database::{DatabaseCollection, DatabaseManager, Error as DbError, MemoryManager, MemoryCollection};
pub use error::Error;
pub use unitary_component::{
    get_default_settings, NotificationHandler, Taple, TapleShutdownManager,
};
