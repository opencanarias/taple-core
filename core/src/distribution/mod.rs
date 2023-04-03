use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    commons::models::notary::NotaryEventResponse,
    identifier::{DigestIdentifier, KeyIdentifier},
    message::TaskCommandContent,
    signature::Signature,
    Event,
};

mod error;
mod inner_manager;
mod manager;
mod resolutor;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DistributionMessages {
    SetEvent(SetEventMessage),
    RequestEvent(RequestEventMessage),
    RequestSignature(RequestSignatureMessage),
    SignaturesReceived(SignaturesReceivedMessage),
}

impl TaskCommandContent for DistributionMessages {}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SetEventMessage {
    pub event: Event,
    pub notaries_signatures: Option<HashSet<NotaryEventResponse>>,
    pub sender: KeyIdentifier,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RequestEventMessage {
    pub subject_id: DigestIdentifier,
    pub sn: u64,
    pub sender: KeyIdentifier,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RequestSignatureMessage {
    pub subject_id: DigestIdentifier,
    // pub governance_id: DigestIdentifier,
    pub namespace: String,
    // pub schema_id: String,
    pub sn: u64,
    pub sender: KeyIdentifier,
    pub requested_signatures: HashSet<KeyIdentifier>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SignaturesReceivedMessage {
    pub subject_id: DigestIdentifier,
    pub sn: u64,
    pub signatures: HashSet<Signature>,
}
