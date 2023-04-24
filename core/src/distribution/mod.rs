use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    commons::models::notary::NotaryEventResponse,
    identifier::{DigestIdentifier, KeyIdentifier},
    signature::Signature,
    Event,
};

pub(crate) mod error;
// mod inner_manager;
// mod manager;
// mod resolutor;

mod manager;
mod inner_manager;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DistributionMessagesNew {
    ProvideSignatures(AskForSignatures),
    SignaturesReceived(SignaturesReceived),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AskForSignatures {
    pub subject_id: DigestIdentifier,
    pub sn: u64,
    pub signatures_requested: HashSet<KeyIdentifier>,
    pub sender_id: KeyIdentifier
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SignaturesReceived {
    pub subject_id: DigestIdentifier,
    pub sn: u64,
    pub signatures: HashSet<Signature>
}

// Message Recieved from Ledger
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StartDistribution {
    subject_id: DigestIdentifier,
    sn: u64
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DistributionMessages {
    SetEvent(SetEventMessage),
    RequestEvent(RequestEventMessage),
    RequestSignature(RequestSignatureMessage),
    SignaturesReceived(SignaturesReceivedMessage),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum LedgerMessages {
    LceRequested(LceRequested),
    EventRequested(EventRequested)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LceRequested {
    pub subject_id: DigestIdentifier,
    pub sender_id: KeyIdentifier,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EventRequested {
    pub subject_id: DigestIdentifier,
    pub sn: u64
}

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
