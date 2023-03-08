use crate::commons::{
    errors::CryptoErrorEvent,
    identifier::{Derivable, DigestIdentifier, KeyIdentifier, SignatureIdentifier},
};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use utoipa::ToSchema;

#[derive(
    Debug,
    Clone,
    Deserialize,
    Serialize,
    PartialEq,
    Hash,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    ToSchema,
    PartialOrd,
)]
pub enum Acceptance {
    Accept,
    Reject,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    ToSchema,
    PartialOrd,
)]
pub struct ApprovalResponse {
    pub content: ApprovalResponseContent,
    #[schema(value_type = String)]
    pub signature: SignatureIdentifier,
}

impl ApprovalResponse {
    pub fn check_signatures(&self) -> Result<(), CryptoErrorEvent> {
        match DigestIdentifier::from_serializable_borsh((
            self.content.event_request_hash.clone(),
            self.content.approval_type.clone(),
            self.content.expected_sn,
        )) {
            Ok(data) => {
                let tmp = self
                    .content
                    .signer
                    .verify(&data.derivative(), self.signature.clone());
                match tmp {
                    Ok(_) => Ok(()),
                    Err(_) => return Err(CryptoErrorEvent::RequestSignatureInvalid),
                }
            }
            Err(_) => return Err(CryptoErrorEvent::EventRequestHashingError),
        }
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, BorshSerialize, BorshDeserialize, ToSchema, PartialOrd,
)]
pub struct ApprovalResponseContent {
    #[schema(value_type = String)]
    pub signer: KeyIdentifier,
    #[schema(value_type = String)]
    pub event_request_hash: DigestIdentifier,
    pub approval_type: Acceptance,
    pub expected_sn: u64,
    pub timestamp: i64,
}

impl PartialEq for ApprovalResponseContent {
    fn eq(&self, other: &Self) -> bool {
        (self.signer == other.signer)
            && (self.event_request_hash == other.event_request_hash)
            && (self.expected_sn == other.expected_sn)
    }
}

impl Hash for ApprovalResponseContent {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.signer.hash(state);
        self.event_request_hash.hash(state);
        self.expected_sn.hash(state);
    }
}
