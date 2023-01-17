use std::{collections::HashSet, iter::FromIterator};

use crate::protocol_message_manager::{Content, EventId, GetMessage, ProtocolManagerMessages};

use commons::identifier::{DigestIdentifier, KeyIdentifier};
use message::{MessageConfig, MessageTaskCommand};

pub fn build_request_event_msg(
    signers: Vec<KeyIdentifier>,
    subject_id: DigestIdentifier,
    sn: u64,
    replication_factor: f64,
) -> MessageTaskCommand<ProtocolManagerMessages> {
    let config = MessageConfig {
        timeout: 0,
        replication_factor,
    };
    MessageTaskCommand::<ProtocolManagerMessages>::Request(
        None,
        ProtocolManagerMessages::GetMessage(GetMessage {
            sn: EventId::SN { sn },
            subject_id,
            request_content: HashSet::from_iter(vec![Content::Event]),
        }),
        Vec::from_iter(signers.into_iter()),
        config,
    )
}

pub fn build_request_signature(
    signature_request: HashSet<KeyIdentifier>, // TODO: It has not been used
    targets: Vec<KeyIdentifier>,
    subject_id: DigestIdentifier,
    sn: u64,
    id: Option<String>,
    replication_factor: f64,
    timeout: u32,
) -> MessageTaskCommand<ProtocolManagerMessages> {
    let config = MessageConfig {
        timeout,
        replication_factor,
    };
    MessageTaskCommand::<ProtocolManagerMessages>::Request(
        id,
        ProtocolManagerMessages::GetMessage(GetMessage {
            sn: EventId::SN { sn },
            subject_id,
            request_content: HashSet::from_iter(vec![Content::Signatures(signature_request)]),
        }),
        targets,
        config,
    )
}

pub fn build_cancel_request(id: String) -> MessageTaskCommand<ProtocolManagerMessages> {
    MessageTaskCommand::<ProtocolManagerMessages>::Cancel(id)
}

pub fn build_request_head(
    targets: Vec<KeyIdentifier>,
    subject_id: DigestIdentifier,
) -> MessageTaskCommand<ProtocolManagerMessages> {
    let config = MessageConfig {
        timeout: 0,
        replication_factor: 1.0,
    };
    MessageTaskCommand::<ProtocolManagerMessages>::Request(
        None,
        ProtocolManagerMessages::GetMessage(GetMessage {
            sn: EventId::HEAD,
            subject_id,
            request_content: HashSet::from_iter(vec![
                Content::Event,
                Content::Signatures(HashSet::new()), // TODO: Raise alternative
            ]),
        }),
        targets,
        config,
    )
}
