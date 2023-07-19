use std::collections::HashSet;

use crate::{
    distribution::{AskForSignatures, DistributionMessagesNew, SignaturesReceived},
    identifier::{DigestIdentifier, KeyIdentifier},
    signature::Signature,
};

use super::approval::TapleMessages;

pub fn create_distribution_request(
    subject_id: DigestIdentifier,
    sn: u64,
    signatures_requested: HashSet<KeyIdentifier>,
    sender_id: KeyIdentifier,
) -> TapleMessages {
    TapleMessages::DistributionMessage(DistributionMessagesNew::ProvideSignatures(
        AskForSignatures {
            subject_id,
            sn,
            signatures_requested,
            sender_id,
        },
    ))
}

pub fn create_distribution_response(
    subject_id: DigestIdentifier,
    sn: u64,
    signatures: HashSet<Signature>,
) -> TapleMessages {
    TapleMessages::DistributionMessage(DistributionMessagesNew::SignaturesReceived(
        SignaturesReceived {
            subject_id,
            sn,
            signatures,
        },
    ))
}
