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
//! use core::{ApiModuleInterface, Taple};
//! use std::{error::Error, time::Duration};
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn Error>> {
//!     let mut settings = Taple::get_default_settings();
//!     settings.node.seed = Some("40000".to_string());
//!     let mut taple = Taple::new(settings);
//!     // The TAPLE node generates several Tokyo tasks to manage the different
//!     // components of its architecture.
//!     // The "start" method initiates these tasks and returns the control flow.
//!     taple.start().await;
//!     // From this point the user can start interacting with the node.
//!     // It is the user's responsibility to decide whether to keep the node running.
//!     // To do so, the main thread of the application must not terminate.
//!     let api = taple.get_api();
//!     // First we need to create the governance, the game set of rules of our future network, to start creating subject on it.
//!     let payload = taple.get_default_governance();
//!     // Next we will send the request to create a governance and we will save the response in a variable for later use.
//!     let response = api
//!         .create_governance(payload)
//!         .await
//!         .expect("Error getting server response");
//!     let subject_id = response
//!         .subject_id
//!         .expect("Error.Response returned empty subject_id");
//!     // Our governance is treated like a subject so, when we create it, inside the response, we have it's subject_id.
//!     // We can use this to retrieve our governance data:
//!     // tokio::time::sleep(Duration::from_secs(1)).await;
//!     let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
//!         "Error getting subject content with id: {}",
//!         subject_id
//!     ));
//!     println!("{:#?}", subject);
//!     // We then give the protocol time to finish its work with a maximum of 4 attempts.
//!     let mut count = 0;
//!     while count != 4 {
//!         if let Ok(data) = api.get_signatures(subject_id.clone(), 0, None, None).await {
//!             if data.len() == 1 {
//!                 break;
//!             }
//!         }
//!         tokio::time::sleep(Duration::from_millis(100)).await;
//!         count += 1;
//!     }
//!     // Now we send a signal to stop our TAPLE node:
//!     tokio::time::sleep(Duration::from_secs(1)).await;
//!     api.shutdown().await;
//!     Ok(())
//! }
//! ```
//!
pub(crate) mod api;
pub mod error;
mod unitary_component;
pub use api::{
    ApiError, ApiModuleInterface, CreateRequest, CreateType, ExternalEventRequest,
    ExternalEventRequestBody, NodeAPI, SignatureRequest, SignatureRequestContent, StateRequestBody,
    StateRequestBodyUpper, StateType,
};
pub use commons::identifier;
pub use commons::models::{event::Event, state::SubjectData};
pub use commons::models::{event_content, event_request, signature};
pub use commons::{
    config::{DatabaseSettings, NetworkSettings, NodeSettings, TapleSettings},
    identifier::derive::{digest::DigestDerivator, KeyDerivator},
    models::notification::Notification,
};
pub use error::Error;
pub use unitary_component::{NotificationHandler, Taple};
