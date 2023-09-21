use std::str::FromStr;

use taple_core::crypto::*;
use taple_core::request::*;
use taple_core::signature::*;
use taple_core::*;
use taple_network_libp2p::network::*;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/**
 * Basic usage of TAPLE Core. It includes:
 * - Node inicialization with on-memory DB (only for testing purpouse)
 * - Minimal governance creation example
 */
#[tokio::main]
async fn main() {
    // Generate ramdom node ID key pair
    let node_key_pair = crypto::KeyPair::Ed25519(Ed25519KeyPair::from_seed(&[]));

    // Configure minimal settings
    let settings = {
        let mut settings = Settings::default();
        settings.node.secret_key = hex::encode(node_key_pair.secret_key_bytes());
        settings
    };

    // Create network
    let (sender, _receiver) = mpsc::channel(10000);
    let (notification_tx, _notification_rx) = mpsc::channel(1000);
    let token = CancellationToken::new();
    let network = NetworkProcessor::new(
        settings.network.listen_addr.clone(),
        network_access_points(&settings.network.known_nodes).unwrap(),
        sender,
        node_key_pair.clone(),
        token,
        notification_tx,
        external_addresses(&settings.network.external_address).unwrap(),
    )
    .expect("Network created");

    // Build node
    let (mut node, api) =
        Node::build(settings, network, MemoryManager::new()).expect("TAPLE node built");

    // Create a minimal governance
    // Compose and sign the subject creation request
    let governance_key = api
        .add_keys(KeyDerivator::Ed25519)
        .await
        .expect("Error getting server response");
    let create_subject_request = EventRequest::Create(StartRequest {
        governance_id: DigestIdentifier::default(),
        name: "".to_string(),
        namespace: "".to_string(),
        schema_id: "governance".to_string(),
        public_key: governance_key,
    });
    let signed_request = Signed::<EventRequest> {
        content: create_subject_request.clone(),
        signature: Signature::new(&create_subject_request, &node_key_pair).unwrap(),
    };

    // Send the signed request to the node
    let _request_id = api.external_request(signed_request).await.unwrap();

    // Wait until event notification
    let subject_id = if let Notification::NewEvent { sn: _, subject_id } = node
        .recv_notification()
        .await
        .expect("NewEvent notification received")
    {
        subject_id
    } else {
        panic!("Unexpected notification");
    };

    // Get the new subject data
    let subject = api
        .get_subject(DigestIdentifier::from_str(&subject_id).unwrap())
        .await
        .unwrap_or_else(|_| panic!("Error getting subject"));

    println!("Subject ID: {}", subject.subject_id.to_str());

    node.shutdown_gracefully().await;
}
