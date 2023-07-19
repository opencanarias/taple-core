mod common;
use common::*;
use futures::FutureExt;
use serial_test::serial;
use std::time::Duration;
use taple_core::{
    event_request::RequestPayload,
    {ApiModuleInterface, CreateType},
};

#[test]
#[serial]
fn invalid_schema_in_policies() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .build();
        let result = node.start().await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let node = node.get_api();
        let result = node
            .create_request(taple_core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(
                    serde_json::to_string(&governance_incorrect_schema_policy()).unwrap(),
                ),
            }))
            .await;
        assert!(result.is_err());
        let result = do_task_with_timeout(node.shutdown().boxed(), 1000).await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn invalid_member_in_policies() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .build();
        let result = node.start().await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let node = node.get_api();
        let result = node
            .create_request(taple_core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(
                    serde_json::to_string(&governance_incorrect_member_in_policy()).unwrap(),
                ),
            }))
            .await;
        assert!(result.is_err());
        let result = do_task_with_timeout(node.shutdown().boxed(), 1000).await;
        assert!(result.is_ok());
    });
}
