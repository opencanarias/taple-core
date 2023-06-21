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
//!use core::{ApiModuleInterface, Taple, identifier::Derivable};
//!use std::{error::Error, time::Duration};
//!use commons::crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial};
//!
//!#[tokio::main]
//!async fn main() -> Result<(), Box<dyn Error>> {
//!    let mut settings = Taple::get_default_settings();
//!    // Generate ramdon cryptographic material
//!    let keypair = Ed25519KeyPair::from_seed(&[]);
//!    let hex_private_key = hex::encode(&keypair.secret_key_bytes());
//!    settings.node.secret_key = Some(hex_private_key);
//!    
//!    let mut taple = Taple::new(settings);
//!    // The TAPLE node generates several Tokyo tasks to manage the different
//!    // components of its architecture.
//!    // The "start" method initiates these tasks and returns the control flow.
//!    taple.start().await.expect("TAPLE started");
//!    // From this point the user can start interacting with the node.
//!    // It is the user's responsibility to decide whether to keep the node running.
//!    // To do so, the main thread of the application must not terminate.
//!    let api = taple.get_api();
//!
//!    // First we need to create the governance, the game set of rules of our future network, to start creating subject on it.
//!    let payload = taple.get_default_governance();
//!
//!    // Next we will send the request to create a governance and we will save the response in a variable for later use.
//!    let response = api
//!        .create_governance(payload)
//!        .await
//!        .expect("Error getting server response");
//!    let subject_id = response
//!        .subject_id
//!        .expect("Error.Response returned empty subject_id");
//!
//!    // wait until validation phase is resolved
//!    let max_attemps = 4;
//!    let mut attemp = 0;
//!    while attemp <= max_attemps {
//!        if let Ok(data) = api.get_signatures(subject_id.clone(), 0, None, None).await {
//!            if data.len() == 1 {
//!                break;
//!            }
//!        }
//!        tokio::time::sleep(Duration::from_millis(100)).await;
//!        attemp += 1;
//!    }
//!    // Our governance is treated like a subject so, when we create it, inside the response, we have it's subject_id.
//!    // We can use this to retrieve our governance data:
//!    let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
//!        "Error getting subject content with id: {}",
//!        subject_id
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
pub(crate) mod commons;
pub mod error;
pub(crate) mod governance;
pub(crate) mod ledger;
pub(crate) mod message;
pub(crate) mod network;
pub(crate) mod approval;
pub(crate) mod database;
pub(crate) mod distribution;
pub(crate) mod evaluator;
pub(crate) mod notary;
pub(crate) mod utils;
pub(crate) mod authorized_subjecs;

pub mod event;
pub mod protocol;

mod unitary_component;
pub use api::{
    APICommands, ApiError, ApiModuleInterface, ApiResponses, GetSubjects, GetEvents,
    GetSubject, NodeAPI,
};
pub use approval::ApprovalPetitionData;
pub use commons::identifier;
pub use commons::crypto;
pub use commons::identifier::{DigestIdentifier, KeyIdentifier, Derivable, SignatureIdentifier};
pub use commons::models::approval::{Approval, ApprovalContent};
pub use commons::models::event::{Event, EventContent};
pub use commons::models::event_proposal::{EventProposal, Evaluation, Proposal};
pub use commons::models::event_request::EventRequestType;
pub use commons::models::Acceptance;
pub use commons::models::state::SubjectData;
pub use commons::models::{event_content, event_request, signature};
pub use commons::models::request;
pub use commons::{
    config::{DatabaseSettings, NetworkSettings, NodeSettings, TapleSettings},
    identifier::derive::{digest::DigestDerivator, KeyDerivator},
    models::notification::Notification,
    models::timestamp::TimeStamp,
    models::event::ValidationProof,
    models::value_wrapper::ValueWrapper,
};
pub use error::Error;
pub use unitary_component::{NotificationHandler, Taple};
pub(crate) use database::DB;
pub use database::{DatabaseCollection, DatabaseManager, Error as DbError, MemoryManager};
