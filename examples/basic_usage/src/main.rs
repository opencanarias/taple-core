use std::str::FromStr;
use std::time::Duration;

use taple_core::ListenAddr;
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
    settings.network.listen_addr = vec![ListenAddr::Memory { port: Some(40040) }];
    settings.node.smartcontracts_directory = path;
    // Generate ramdon cryptographic material
    let keypair = Ed25519KeyPair::from_seed(&[0; 32]);
    let hex_private_key = hex::encode(&keypair.secret_key_bytes());
    settings.node.secret_key = Some(hex_private_key);
    let kp = crypto::KeyPair::Ed25519(keypair);
    let public_key = kp.public_key_bytes();
    let key_identifier = KeyIdentifier::new(kp.get_key_derivator(), &public_key);

    // We call the constructor method of a TAPLE node
    let mut taple = Taple::new(settings, MemoryManager::new());
    // We can use the next structure to get notifications from the node
    let mut notifier = taple.get_notification_handler();
    // The TAPLE node generates several Tokyo tasks to manage the different
    // components of its architecture.
    // The "start" method initiates these tasks and returns the control flow.
    let shutdown_manager = taple.get_shutdown_manager();
    taple.start().await.expect("TAPLE started");
    // From this point the user can start interacting with the node.
    // It is the user's responsibility to decide whether to keep the node running.
    // To do so, the main thread of the application must not terminate.
    let api = taple.get_api();

    // First we need to add the keys for our Subject
    let pk_added = api
        .add_keys(KeyDerivator::Ed25519)
        .await
        .expect("Error getting server response");

    // Create the request
    let event_request = EventRequest::Create(StartRequest {
        governance_id: DigestIdentifier::default(),
        name: "Paco23".to_string(),
        namespace: "".to_string(),
        schema_id: "governance".to_string(),
        public_key: pk_added,
    });

    let signature = Signature::new(&event_request, key_identifier, &kp).unwrap();

    let er_signed = Signed::<EventRequest> {
        content: event_request.clone(),
        signature,
    };

    // We need to create the governance, the game set of rules of our future network, to start creating subject on it.
    let _request_id = api.external_request(er_signed).await.unwrap();
    // We wait until the notification of a new subject created is received
    let subject_id = tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(5000)) => {
            panic!("Max time waited");
        },
        subject_id = async {
            loop {
                match notifier.receive().await {
                    Ok(data) => {
                        if let Notification::NewSubject { subject_id } = data {
                            break subject_id;
                        }
                    },
                    Err(_) => {
                        panic!("Notifier error")
                    }
                }
            }
        } => subject_id
    };

    // Next, we convert the subject_id to a DigestIdentifier
    let subject_id = DigestIdentifier::from_str(&subject_id).expect("DigestIdentifier from str failed");

    // We use this ID to retrieve governance data
    let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
        "Error getting subject content with id: {}",
        subject_id.to_str()
    ));

    println!("Governance subject Id: {:#?}", subject.subject_id.to_str());
    println!("Governance subject SN: {:#?}", subject.sn);

    // Now we send a signal to stop our TAPLE node:
    shutdown_manager.shutdown().await;
    Ok(())
}
