use std::str::FromStr;

use taple_core::crypto;
use taple_core::crypto::Ed25519KeyPair;
use taple_core::crypto::KeyGenerator;
use taple_core::crypto::KeyMaterial;
use taple_core::get_default_settings;
use taple_core::request::StartRequest;
use taple_core::signature::Signature;
use taple_core::signature::Signed;
use taple_core::ApiModuleInterface;
use taple_core::Derivable;
use taple_core::DigestIdentifier;
use taple_core::EventRequest;
use taple_core::KeyDerivator;
use taple_core::KeyIdentifier;
use taple_core::MemoryManager;
use taple_core::Notification;
use taple_core::Taple;

/**
 * Basic usage of TAPLE Core. It includes:
 * - Node inicialization with on-memory DB (only for testing purpouse)
 * - Minimal governance creation example
 */
#[tokio::main]
async fn main() -> Result<(), ()> {
    // Generate ramdom node cryptographic material and ID
    let node_key_pair = crypto::KeyPair::Ed25519(Ed25519KeyPair::from_seed(&[]));
    let node_identifier = KeyIdentifier::new(
        node_key_pair.get_key_derivator(),
        &node_key_pair.public_key_bytes(),
    );

    // Build and start up the TAPLE node
    let mut settings = get_default_settings();
    settings.node.secret_key = Some(hex::encode(&node_key_pair.secret_key_bytes()));

    let mut taple = Taple::new(settings, MemoryManager::new());
    let mut notifier = taple.get_notification_handler();
    let shutdown_manager = taple.get_shutdown_manager();
    let api = taple.get_api();

    taple.start().await.expect("TAPLE started");

    // Create a minimal governance
    // Compose and sign the subject creation request
    let new_key = api
        .add_keys(KeyDerivator::Ed25519)
        .await
        .expect("Error getting server response");

    let create_subject_request = EventRequest::Create(StartRequest {
        governance_id: DigestIdentifier::default(),
        name: "".to_string(),
        namespace: "".to_string(),
        schema_id: "governance".to_string(),
        public_key: new_key,
    });

    let signed_request = Signed::<EventRequest> {
        content: create_subject_request.clone(),
        signature: Signature::new(&create_subject_request, node_identifier, &node_key_pair)
            .unwrap(),
    };

    // Send the signed request to the node
    let _request_id = api.external_request(signed_request).await.unwrap();

    // Wait until notification of subject creation
    let subject_id =
        if let Notification::NewSubject { subject_id } = notifier.receive().await.unwrap() {
            subject_id
        } else {
            panic!("Unexpected notification");
        };

    // Get the new subject data
    let subject_id =
        DigestIdentifier::from_str(&subject_id).expect("DigestIdentifier from str failed");

    let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
        "Error getting subject content with id: {}",
        subject_id.to_str()
    ));

    println!("{:#?}", subject);

    // Shutdown the TAPLE node
    shutdown_manager.shutdown().await;

    Ok(())
}
