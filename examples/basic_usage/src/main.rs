use std::str::FromStr;

use taple_core::Notification;
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
use taple_core::Taple;

#[tokio::main]
async fn main() -> Result<(), ()> {
    let tmp_path = std::env::temp_dir();
    let path = format!("{}/.taple/contracts", tmp_path.display());
    std::fs::create_dir_all(&path).expect("Contract folder not created");

    let mut settings = get_default_settings();

    // Generate ramdon cryptographic material
    let keypair = Ed25519KeyPair::from_seed(&[0; 32]);
    let hex_private_key = hex::encode(&keypair.secret_key_bytes());
    settings.node.secret_key = Some(hex_private_key);
    let kp = crypto::KeyPair::Ed25519(keypair);
    let public_key = kp.public_key_bytes();
    let key_identifier = KeyIdentifier::new(kp.get_key_derivator(), &public_key);

    // We build the TAPLE node
    let mut taple = Taple::new(settings, MemoryManager::new());
    // We obtain the notification channel
    let mut notifier = taple.get_notification_handler();
    // We obtain the shutdown manager
    let shutdown_manager = taple.get_shutdown_manager();
    // We initialize the node
    taple.start().await.expect("TAPLE started");
    let api = taple.get_api();

    // The minimal step in TAPLE is to create a governance

    // First we need to add the keys for our governance, which internally it is a subject
    let pk_added = api
        .add_keys(KeyDerivator::Ed25519)
        .await
        .expect("Error getting server response");

    // Create the request
    let event_request = EventRequest::Create(StartRequest {
        governance_id: DigestIdentifier::default(),
        name: "".to_string(),
        namespace: "".to_string(),
        schema_id: "governance".to_string(),
        public_key: pk_added,
    });

    // We sign the request
    let signature = Signature::new(&event_request, key_identifier, &kp).unwrap();
    let er_signed = Signed::<EventRequest> {
        content: event_request.clone(),
        signature,
    };

    // We send the request to the node
    let _request_id = api.external_request(er_signed).await.unwrap();

    // We wait until the notification of a new subject created is received

    let subject_id = match notifier.receive().await {
        Ok(data) => {
            if let Notification::NewSubject { subject_id } = data {
                subject_id
            } else {
                panic!("Unexpected notification");
            }
        },
        Err(_) => {
            panic!("Notifier error")
        }
    };

    // Next, we convert the subject_id to a DigestIdentifier
    let subject_id = DigestIdentifier::from_str(&subject_id).expect("DigestIdentifier from str failed");

    // We use this ID to retrieve governance data
    let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
        "Error getting subject content with id: {}",
        subject_id.to_str()
    ));

    // We print the governance received
    println!("{:#?}", subject);

    // Now we send a signal to stop our TAPLE node:
    shutdown_manager.shutdown().await;
    Ok(())
}
