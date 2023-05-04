use crate::{
    distribution::{EventRequested, LceRequested, LedgerMessages},
    identifier::{DigestIdentifier, KeyIdentifier},
    ledger::LedgerCommand,
};

use super::approval::TapleMessages;

pub fn request_lce(who_asked: KeyIdentifier, subject_id: DigestIdentifier) -> TapleMessages {
    TapleMessages::LedgerMessages(LedgerCommand::GetLCE {
        who_asked,
        subject_id,
    })
}

pub fn request_event(
    who_asked: KeyIdentifier,
    subject_id: DigestIdentifier,
    sn: u64,
) -> TapleMessages {
    TapleMessages::LedgerMessages(LedgerCommand::GetEvent {
        who_asked,
        subject_id,
        sn,
    })
}
