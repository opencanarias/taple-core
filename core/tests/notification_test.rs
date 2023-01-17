mod common;
use std::time::Duration;

use common::*;
use core::{ApiModuleInterface, event_request::RequestPayload};

#[test]
fn notification_test() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut node = NodeBuilder::new()
            .with_addr("/memory".into())
            .with_p2p_port(40000)
            .with_timeout(100)
            .with_seed("40000".into())
            .build();
        node.start().await.unwrap();
        let mut notification_handler = node.get_notification_handler();
        let node = node.get_api();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let result = node
            .create_governance(RequestPayload::Json(
                serde_json::to_string(&governance_one()).unwrap(),
            ))
            .await;
        assert!(result.is_ok());
        let gov_id = result.unwrap().subject_id.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        // There should be 3 notifications: Signature Event, Subject Creation and Event arrives to Quorum
        let result = notification_handler.try_rec();
        assert_eq!(
            result.unwrap().to_message(),
            format!("Evento 0 del sujeto {} firmado", gov_id)
        );
        let result = notification_handler.try_rec();
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().to_message(),
            format!("Sujeto {} creado", gov_id)
        );
        let result = notification_handler.try_rec();
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().to_message(),
            format!("Evento 0 del sujeto {} ha llegado a Quorum", gov_id)
        );
        let result = notification_handler.try_rec();
        assert!(result.is_err());
        let result = node.shutdown().await;
        assert!(result.is_ok());
    });
}
