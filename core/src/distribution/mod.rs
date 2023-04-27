use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    identifier::{DigestIdentifier, KeyIdentifier},
    signature::Signature,
};

pub(crate) mod error;
pub(crate) mod inner_manager;
pub(crate) mod manager;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DistributionMessagesNew {
    ProvideSignatures(AskForSignatures),
    SignaturesReceived(SignaturesReceived),
    SignaturesNeeded {
        subject_id: DigestIdentifier,
        sn: u64,
    },
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AskForSignatures {
    pub subject_id: DigestIdentifier,
    pub sn: u64,
    pub signatures_requested: HashSet<KeyIdentifier>,
    pub sender_id: KeyIdentifier,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SignaturesReceived {
    pub subject_id: DigestIdentifier,
    pub sn: u64,
    pub signatures: HashSet<Signature>,
}

// Message Recieved from Ledger
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StartDistribution {
    subject_id: DigestIdentifier,
    sn: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum LedgerMessages {
    LceRequested(LceRequested),
    EventRequested(EventRequested),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LceRequested {
    pub subject_id: DigestIdentifier,
    pub sender_id: KeyIdentifier,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EventRequested {
    pub subject_id: DigestIdentifier,
    pub sn: u64,
}
