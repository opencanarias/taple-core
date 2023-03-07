mod common;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use core::{
    {ApiModuleInterface, CreateType, StateType, Acceptance},
    event_request::RequestPayload, 
};
use futures::FutureExt;
use serial_test::serial;

#[test]
#[serial]
fn init_node() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        std::env::set_var("RUST_LOG", "info");
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        let result = node.start().await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let node = node.get_api();
        let result = do_task_with_timeout(node.shutdown().boxed(), 1000).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn database_persistence() {
    let _ = std::fs::remove_dir_all(std::path::Path::new("/tmp/data"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        std::env::set_var("RUST_LOG", "info");
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .with_database_path("/tmp/data".into())
            .build();
        node.start().await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let node = node.get_api();
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_one()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_millis(200)).await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node.shutdown().await;
        assert!(result.is_ok());
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .with_database_path("/tmp/data".into())
            .build();
        node.start().await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let node = node.get_api();
        let result = node.get_subject(governance_id.clone()).await;
        assert!(result.is_ok());
        let result = node.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn not_database_conflict() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let node = node.get_api();
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .build();
        node_two.start().await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let node_two = node_two.get_api();
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            }))
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node.get_subject(governance_id.clone()).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node.shutdown().await;
        assert!(result.is_ok());
        let result = node_two.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn event_creation_json_patch() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_pass_votation(1)
            .with_dev_mode(true)
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node
            .create_subject(
                governance_id.clone(),
                "prueba".into(),
                "".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({"a": "test"})).unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let subject_id = result.unwrap().subject_id.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_event(
                subject_id.clone(),
                RequestPayload::JsonPatch(String::from("[{\"op\":\"replace\",\"path\":\"/a\",\"value\":\"test\"}]")),
            )
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_millis(100)).await;
        let result = node.get_subject(subject_id).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.sn, 1);
        let result = node.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn governance_transmission() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node.get_subject(governance_id.clone()).await;
        assert!(result.is_ok());
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let result = get_subject_with_timeout(node_two.clone(), governance_id.clone(), 5000).await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 2, 5000).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
        let result =
            get_signatures_with_timeout(node_two.clone(), governance_id.clone(), 0, 2, 5000).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn get_pending_request() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .build();
        let result = node.start().await;
        assert!(result.is_ok());
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let gov_one = result.unwrap().subject_id.unwrap();
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let gov_two = result.unwrap().subject_id.unwrap();
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let gov_three = result.unwrap().subject_id.unwrap();
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let result = get_subject_with_timeout(node_two.clone(), gov_one.clone(), 5000).await;
        assert!(result.is_ok());
        let result = get_subject_with_timeout(node_two.clone(), gov_two.clone(), 5000).await;
        assert!(result.is_ok());
        let result = get_subject_with_timeout(node_two.clone(), gov_three.clone(), 5000).await;
        assert!(result.is_ok());
        let result = node
            .create_request(core::CreateRequest::State(StateType {
                subject_id: gov_one,
                payload: RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let result = node
            .create_request(core::CreateRequest::State(StateType {
                subject_id: gov_two,
                payload: RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let result = node
            .create_request(core::CreateRequest::State(StateType {
                subject_id: gov_three,
                payload: RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let result = node_two.get_pending_requests().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn governance_creation_failed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&serde_json::json!({})).unwrap(),
            ))
            .await;
        assert!(result.is_err());
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&serde_json::json!({
                        "members_wrong": [
                            {
                                "id": "Open Canarias",
                                "tags": {},
                                "description": "a",
                                "key": "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                            },
                        ],
                        "schemas": [
                            {
                                "id": "prueba",
                                "tags": {},
                                "content": {"type": "string"}
                            }
                        ]
                }))
                .unwrap(),
            ))
            .await;
        assert!(result.is_err());
        let result = node.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn subject_creation_failed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let governance_id = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await
            .unwrap()
            .subject_id
            .unwrap();
        // Invalid governance_id
        let result = node
            .create_subject(
                "".into(),
                "prueba".into(),
                "".into(),
                RequestPayload::Json(serde_json::to_string(&serde_json::json!("69")).unwrap()),
            )
            .await;
        assert!(result.is_err());
        // Invalid schema_id
        let result = node
            .create_subject(
                governance_id.clone(),
                "invalid".into(),
                "".into(),
                RequestPayload::Json(serde_json::to_string(&serde_json::json!("69")).unwrap()),
            )
            .await;
        assert!(result.is_err());
        // Invalid Payload
        let result = node
            .create_subject(
                governance_id.clone(),
                "prueba".into(),
                "".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "invalid": "69"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_err());
        let result = node.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn subject_creation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_two()).unwrap(),
            ))
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node_two
            .create_subject(
                governance_id.clone(),
                "prueba".into(),
                "".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "69"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let subject_id = result.unwrap().subject_id.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node.get_subject(subject_id.clone()).await;
        assert!(result.is_ok());
        let result = node
            .get_event_of_subject(subject_id.clone(), Some(0), Some(0))
            .await;
        assert!(result.is_ok());
        let result = node_two
            .get_signatures(subject_id.clone(), 0, None, None)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
        let result = node.shutdown().await;
        assert!(result.is_ok());
        let result = node_two.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn event_creation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_two()).unwrap(),
            ))
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node_two
            .create_subject(
                governance_id.clone(),
                "prueba".into(),
                "namespace1".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "69"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let subject_id = result.unwrap().subject_id.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node_two
            .create_event(
                subject_id.clone(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "70"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .get_event_of_subject(subject_id.clone(), Some(1), Some(1))
            .await;
        assert!(result.is_ok());
        let result = node.get_signatures(subject_id.clone(), 1, None, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
        let result = node_two
            .create_event(
                subject_id.clone(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "71"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .get_event_of_subject(subject_id.clone(), Some(2), Some(2))
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
        let result = node_two
            .get_signatures(subject_id.clone(), 2, None, None)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
        let result = node.get_subject(subject_id).await;
        assert_eq!(result.unwrap().sn, 2);
        let result = node.shutdown().await;
        assert!(result.is_ok());
        let result = node_two.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn event_creation_case_100_quorum_and_not_self_validation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_two_100()).unwrap(),
            ))
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node
            .create_subject(
                governance_id.clone(),
                "prueba".into(),
                "namespace1".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "69"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let subject_id = result.unwrap().subject_id.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let result = node
            .create_event(
                subject_id.clone(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "70"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let sn_first = node.get_subject(subject_id.clone()).await;
        assert!(sn_first.is_ok());
        let sn_first = sn_first.unwrap().sn;
        let sn_second = node_two.get_subject(subject_id.clone()).await;
        assert!(sn_second.is_ok());
        let sn_second = sn_second.unwrap().sn;
        assert_eq!(sn_first, sn_second);
        let result = node.shutdown().await;
        assert!(result.is_ok());
        let result = node_two.shutdown().await;
        assert!(result.is_ok());
    });
}


#[test]
#[serial]
fn event_creation_not_allowed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_two()).unwrap(),
            ))
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node_two
            .create_subject(
                governance_id.clone(),
                "prueba".into(),
                "".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "test"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let subject_id = result.unwrap().subject_id.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_event(
                subject_id.clone(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "test-2"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_err());
        let result = node.shutdown().await;
        assert!(result.is_ok());
        let result = node_two.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn event_creation_failed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node
            .create_subject(
                governance_id.clone(),
                "prueba".into(),
                "".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({"a": "test"})).unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let subject_id = result.unwrap().subject_id.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        // Bad subject ID
        let result = node
            .create_event(
                "invalid".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({"a": "test2"})).unwrap(),
                ),
            )
            .await;
        assert!(result.is_err());
        // Bad Payload
        let result = node
            .create_event(
                subject_id,
                RequestPayload::Json(serde_json::to_string(&serde_json::json!(4)).unwrap()),
            )
            .await;
        assert!(result.is_err());
        let result = node.shutdown().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn add_new_member_to_governance() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 1, 5000).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        let id = result.unwrap().request_id;
        let result = node.approval_request(id, Acceptance::Accept).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 1, 2, 1000).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sn, 1);
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn add_new_member_to_governance_all_acceptance_true() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 1, 5000).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_two_allowance_all_false()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        let id = result.unwrap().request_id;
        let result = node.approval_request(id, Acceptance::Accept).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 1, 2, 1000).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sn, 1);
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

// #[test]
// fn add_new_member_to_governance_051_quorum() {
//     let rt = tokio::runtime::Runtime::new().unwrap();
//     rt.block_on(async {
//         let mut node = NodeBuilder::new()
//             .with_addr("/memory".into())
//             .with_p2p_port(40000)
//             .with_seed("40000".into())
//             .with_timeout(100)
//             .build();
//         node.start().await.unwrap();
//         let node = node.get_api();
//         tokio::time::sleep(Duration::from_secs(1)).await;
//         let mut node_two = NodeBuilder::new()
//             .with_addr("/memory".into())
//             .with_p2p_port(40001)
//             .with_seed("40001".into())
//             .with_timeout(100)
//             .add_access_point(
//                 "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
//             )
//             .build();
//         node_two.start().await.unwrap();
//         let node_two = node_two.get_api();
//         tokio::time::sleep(Duration::from_secs(1)).await;
//         let result = node
//             .create_governance(RequestPayload::Json(
//                 serde_json::to_string(&governance_one()).unwrap(),
//             ))
//             .await;
//         let governance_id = result.unwrap().subject_id.unwrap();
//         let node = Arc::new(node);
//         let node_two = Arc::new(node_two);
//         let result =
//             get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 1, 5000).await;
//         assert!(result.is_ok());
//         let result = node_two.get_subject(governance_id.clone()).await;
//         assert!(result.is_err());
//         let result = node
//             .create_event(
//                 governance_id.clone(),
//                 RequestPayload::Json(serde_json::to_string(&governance_two_051()).unwrap()),
//             )
//             .await;
//         assert!(result.is_ok());
//         let id = result.unwrap().request_id;
//         let result = node.approval_request(id, Acceptance::Accept).await;
//         tokio::time::sleep(Duration::from_secs(1)).await;
//         assert!(result.is_ok());
//         let result =
//             get_signatures_with_timeout(node.clone(), governance_id.clone(), 1, 2, 1000).await;
//         tokio::time::sleep(Duration::from_secs(1)).await;
//         assert!(result.is_ok());
//         let result = node_two.get_subject(governance_id.clone()).await;
//         assert!(result.is_ok());
//         assert_eq!(result.unwrap().sn, 1);
//         let result = Arc::try_unwrap(node).unwrap().shutdown().await;
//         assert!(result.is_ok());
//         let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
//         assert!(result.is_ok());
//     });
// }

#[test]
#[serial]
fn add_new_member_to_governance_approval_failed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 1, 5000).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        let id = result.unwrap().request_id;
        let result = node.approval_request(id.clone(), Acceptance::Reject).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 1, 2, 1000).await;
        assert!(result.is_err());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node.approval_request(id, Acceptance::Accept).await;
        assert!(result.is_err());
        let result = node.get_subject(governance_id.clone()).await;
        assert_eq!(result.unwrap().sn, 1);
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn add_new_member_to_governance_two_at_the_start() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_three = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40002)
            .with_seed("40002".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_three.start().await.unwrap();
        let node_three = node_three.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_two()).unwrap(),
            ))
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let node_three = Arc::new(node_three);
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 2, 5000).await;
        assert!(result.is_ok());
        let result = node_three.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_three()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        let id = result.unwrap().request_id;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let result = node.approval_request(id.clone(), Acceptance::Accept).await;
        assert!(result.is_ok());
        let result = node_two.approval_request(id, Acceptance::Accept).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node_three.clone(), governance_id.clone(), 1, 3, 5000)
                .await;
        assert!(result.is_ok());
        let result = node_three.get_subject(governance_id.clone()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sn, 1);
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_three).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn add_new_schema_to_governance() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        let governance_id = result.unwrap().subject_id.unwrap();
        // Invalid schema
        let result = node
            .create_subject(
                governance_id.clone(),
                "prueba-2".into(),
                "".into(),
                RequestPayload::Json(serde_json::to_string(&serde_json::json!(1453)).unwrap()),
            )
            .await;
        assert!(result.is_err());
        // Updating governance
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                            "members": [
                                {
                                    "id": "Open Canarias",
                                    "tags": {},
                                    "description": "a",
                                    "key": "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                                },
                            ],
                            "schemas": [
                                {
                                    "id": "prueba",
                                    "tags": {},
                                    "content": {"type": "string"}
                                },
                                {
                                    "id": "prueba-2",
                                    "tags": {},
                                    "content": {"type": "number"}
                                }
                            ],
                            "policies": [
                                {
                                    "id": "prueba",
                                    "validation": {
                                        "quorum": 0.5,
                                        "validators": [
                                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                                        ]
                                    },
                                    "approval": {
                                        "quorum": 0.5,
                                        "approvers": [
                                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                                        ]
                                    },
                                    "invokation": {
                                        "owner": {
                                            "allowance": true,
                                            "approvalRequired": true
                                        },
                                        "set": {
                                            "allowance": false,
                                            "approvalRequired": false,
                                            "invokers": []
                                        },
                                        "all": {
                                            "allowance": false,
                                            "approvalRequired": false,
                                        },
                                        "external": {
                                            "allowance": false,
                                            "approvalRequired": false
                                        }
                                    }
                                },
                                {
                                    "id": "governance",
                                    "validation": {
                                        "quorum": 0.5,
                                        "validators": [
                                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                                        ]
                                    },
                                    "approval": {
                                        "quorum": 0.5,
                                        "approvers": [
                                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                                        ]
                                    },
                                    "invokation": {
                                        "owner": {
                                            "allowance": true,
                                            "approvalRequired": true
                                        },
                                        "set": {
                                            "allowance": false,
                                            "approvalRequired": false,
                                            "invokers": []
                                        },
                                        "all": {
                                            "allowance": true,
                                            "approvalRequired": true,
                                        },
                                        "external": {
                                            "allowance": false,
                                            "approvalRequired": false
                                        }
                                    }
                                },
                                {
                                    "id": "prueba-2",
                                    "validation": {
                                        "quorum": 0.5,
                                        "validators": [
                                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                                        ]
                                    },
                                    "approval": {
                                        "quorum": 0.5,
                                        "approvers": [
                                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                                        ]
                                    },
                                    "invokation": {
                                        "owner": {
                                            "allowance": true,
                                            "approvalRequired": true
                                        },
                                        "set": {
                                            "allowance": false,
                                            "approvalRequired": false,
                                            "invokers": []
                                        },
                                        "all": {
                                            "allowance": false,
                                            "approvalRequired": false,
                                        },
                                        "external": {
                                            "allowance": false,
                                            "approvalRequired": false
                                        }
                                    }
                                },
                            ]
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let id = result.unwrap().request_id;
        let result = node.approval_request(id, Acceptance::Accept).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        // New Subject with new schema
        let result = node
            .create_subject(
                governance_id.clone(),
                "prueba-2".into(),
                "".into(),
                RequestPayload::Json(serde_json::to_string(&serde_json::json!(1453)).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node.shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn synchronization_after_added_to_governance() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(200)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(200)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let result = node
            .create_subject(
                governance_id.clone(),
                "prueba".into(),
                "".into(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "69"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let node = Arc::new(node);
        let subject_id = result.unwrap().subject_id.unwrap();
        let result =
            get_signatures_with_timeout(node.clone(), subject_id.clone(), 0, 1, 5000).await;
        assert!(result.is_ok());
        let result = node
            .create_event(
                subject_id.clone(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "70"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node.clone(), subject_id.clone(), 1, 1, 5000).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
        let result = node
            .create_event(
                subject_id.clone(),
                RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "71"
                    }))
                    .unwrap(),
                ),
            )
            .await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node.clone(), subject_id.clone(), 2, 1, 5000).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);

        // Updating governance to add new members
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        std::env::set_var("RUST_LOG", "info");
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 1, 2, 5000).await;
        assert!(result.is_ok());

        tokio::time::sleep(Duration::from_secs(2)).await;
        let result = node.get_subject(subject_id.clone()).await;
        assert!(result.is_ok());
        let sn = result.unwrap().sn;
        assert_eq!(sn, 2);

        let result = node_two
            .get_event_of_subject(subject_id.clone(), Some(0), Some(3))
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);

        let result = node_two.get_subject(subject_id.clone()).await;
        assert!(result.is_ok());
        let sn = result.unwrap().sn;
        assert_eq!(sn, 2);

        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = node_two.shutdown().await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
}

#[test]
#[serial]
fn test_approval_pass_with_accept() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .with_pass_votation(1)
            .with_dev_mode(true)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_pass_votation(1)
            .with_timeout(100)
            .with_dev_mode(true)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 1, 5000).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 1, 2, 2000).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sn, 1);
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn test_approval_pass_with_reject() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .with_pass_votation(2)
            .with_dev_mode(true)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_pass_votation(2)
            .with_timeout(100)
            .with_dev_mode(true)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        let governance_id = result.unwrap().subject_id.unwrap();
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 1, 5000).await;
        assert!(result.is_ok());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_two()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 1, 2, 2000).await;
        assert!(result.is_err());
        let result = node_two.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn test_get_rejected_event() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut node_two = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40001)
            .with_seed("40001".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_two.start().await.unwrap();
        let node_two = node_two.get_api();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut node_three = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40002)
            .with_seed("40002".into())
            .with_timeout(100)
            .add_access_point(
                "/memory/40000/p2p/12D3KooWBGEMfdAeRHp5eZ1zTpyEeZyvJYoBrDo9WLEtjWZWnCwD".into(),
            )
            .build();
        node_three.start().await.unwrap();
        let node_three = node_three.get_api();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_two()).unwrap(),
            ))
            .await;
        assert!(result.is_ok());
        let governance_id = result.unwrap().subject_id.unwrap();
        let node = Arc::new(node);
        let node_two = Arc::new(node_two);
        let node_three = Arc::new(node_three);
        let result =
            get_signatures_with_timeout(node.clone(), governance_id.clone(), 0, 2, 5000).await;
        assert!(result.is_ok());
        let result = node_three.get_subject(governance_id.clone()).await;
        assert!(result.is_err());
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_three()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        let id = result.unwrap().request_id;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let result = node_two
            .approval_request(id.clone(), Acceptance::Reject)
            .await;
        assert!(result.is_ok());
        let result = node.approval_request(id.clone(), Acceptance::Reject).await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node_three.clone(), governance_id.clone(), 1, 3, 1000)
                .await;
        assert!(result.is_err());
        let result =
            get_signatures_with_timeout(node_two.clone(), governance_id.clone(), 1, 2, 1000).await;
        assert!(result.is_ok());
        let result = node
            .create_event(
                governance_id.clone(),
                RequestPayload::Json(serde_json::to_string(&governance_three()).unwrap()),
            )
            .await;
        assert!(result.is_ok());
        let id = result.unwrap().request_id;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let result = node_two
            .approval_request(id.clone(), Acceptance::Accept)
            .await;
        assert!(result.is_ok());
        let result = node.approval_request(id, Acceptance::Accept).await;
        assert!(result.is_ok());
        let result =
            get_signatures_with_timeout(node_three.clone(), governance_id.clone(), 2, 3, 1000)
                .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(result.is_ok());
        let result = node_three.get_subject(governance_id.clone()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().sn, 2);
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_two).unwrap().shutdown().await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node_three).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn test_create_governance_request() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_one()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.sn.is_some());
        assert!(result.subject_id.is_some());
        let governance_id = result.subject_id.unwrap();
        let node = Arc::new(node);
        let result = get_signatures_with_timeout(node.clone(), governance_id, 0, 1, 1000).await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn test_create_subject_request() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_one()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.sn.is_some());
        assert!(result.subject_id.is_some());
        let governance_id = result.subject_id.unwrap();

        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: governance_id,
                schema_id: "prueba".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "123"
                    }))
                    .unwrap(),
                ),
            }))
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.sn.is_some());
        assert!(result.subject_id.is_some());
        let subject_id = result.subject_id.unwrap();
        let node = Arc::new(node);
        let result = get_signatures_with_timeout(node.clone(), subject_id, 0, 1, 1000).await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}

#[test]
#[serial]
fn test_update_subject_request() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_seed("40000".into())
            .with_timeout(100)
            .with_dev_mode(true)
            .with_pass_votation(1)
            .build();
        node.start().await.unwrap();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: "".into(),
                schema_id: "governance".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(serde_json::to_string(&governance_one()).unwrap()),
            }))
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.sn.is_some());
        assert!(result.subject_id.is_some());
        let governance_id = result.subject_id.unwrap();

        let result = node
            .create_request(core::CreateRequest::Create(CreateType {
                governance_id: governance_id,
                schema_id: "prueba".into(),
                namespace: "".into(),
                payload: RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "123"
                    }))
                    .unwrap(),
                ),
            }))
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        let subject_id = result.subject_id.unwrap();

        let result = node
            .create_request(core::CreateRequest::State(StateType {
                subject_id: subject_id.clone(),
                payload: RequestPayload::Json(
                    serde_json::to_string(&serde_json::json!({
                        "a": "130"
                    }))
                    .unwrap(),
                ),
            }))
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.sn.is_none());
        let node = Arc::new(node);
        let result = get_signatures_with_timeout(node.clone(), subject_id, 1, 1, 1000).await;
        assert!(result.is_ok());
        let result = Arc::try_unwrap(node).unwrap().shutdown().await;
        assert!(result.is_ok());
    });
}
