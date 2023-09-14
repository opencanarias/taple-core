mod common;
use common::{generate_mc, NodeBuilder};
use serial_test::serial;

use crate::common::{check_subject, create_governance_request};

#[test]
#[serial]
fn init_node() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        std::env::set_var("RUST_LOG", "info");
        let mc_data_node1 = generate_mc();
        let mut node = NodeBuilder::new(mc_data_node1.get_private_key()).build();
        let result = node.start().await;
        assert!(result.is_ok());
        node.shutdown().await
    });
}

#[test]
#[serial]
fn create_governance() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mc_data_node1 = generate_mc();
        let mut node = NodeBuilder::new(mc_data_node1.get_private_key()).build();
        let result = node.start().await;
        assert!(result.is_ok());
        let node_api = node.get_api();
        let public_key = node_api
            .add_keys(taple_core::KeyDerivator::Ed25519)
            .await
            .expect("MC creation failed");
        let event_request = create_governance_request("", public_key, "");
        assert!(node_api
            .external_request(mc_data_node1.sign_event_request(&event_request))
            .await
            .is_ok());
        // Wait for the subject creation notification
        let result = node.wait_for_new_subject().await;
        assert!(result.is_ok());
        let subject_id = result.unwrap();
        // Check the subject asking the api about it
        check_subject(&node_api, &subject_id, Some(0)).await;
        node.shutdown().await
    });
}
