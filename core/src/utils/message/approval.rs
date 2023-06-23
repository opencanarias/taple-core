pub use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{signature::Signed, Proposal};

pub fn create_approval_request(event_proposal: Signed<Proposal>) -> TapleMessages {
    TapleMessages::ApprovalMessages(crate::approval::ApprovalMessages::RequestApproval(
        event_proposal,
    ))
}
