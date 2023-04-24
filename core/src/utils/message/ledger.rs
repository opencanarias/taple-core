use crate::{
    distribution::{LceRequested, LedgerMessages},
    identifier::{DigestIdentifier, KeyIdentifier},
};

use super::approval::TapleMessages;

pub fn request_lce(subject_id: DigestIdentifier, sender_id: KeyIdentifier) -> TapleMessages {
    TapleMessages::LedgerMessages(LedgerMessages::LceRequested(LceRequested {
        subject_id,
        sender_id,
    }))
}
