use crate::{
    distribution::{EventRequested, LceRequested, LedgerMessages},
    identifier::{DigestIdentifier, KeyIdentifier},
};

use super::approval::TapleMessages;

pub fn request_lce(subject_id: DigestIdentifier) -> TapleMessages {
    TapleMessages::LedgerMessages(LedgerMessages::LceRequested(LceRequested {
        subject_id,
    }))
}

pub fn request_event(subject_id: DigestIdentifier, sn: u64) -> TapleMessages {
    TapleMessages::LedgerMessages(LedgerMessages::EventRequested(EventRequested {
        subject_id,
        sn,
    }))
}
