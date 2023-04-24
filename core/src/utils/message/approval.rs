use crate::commons::models::event_proposal::{EventProposal};
pub use crate::protocol::protocol_message_manager::TapleMessages;

pub fn create_approval_request(event_proposal: EventProposal) -> TapleMessages {
    TapleMessages::ApprovalMessages(crate::approval::ApprovalMessages::RequestApproval(
        event_proposal,
    ))
}
