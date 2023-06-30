pub use crate::protocol::protocol_message_manager::TapleMessages;
use crate::{signature::Signed, ApprovalRequest};

pub fn create_approval_request(event_proposal: Signed<ApprovalRequest>) -> TapleMessages {
    TapleMessages::ApprovalMessages(crate::approval::ApprovalMessages::RequestApproval(
        event_proposal,
    ))
}
