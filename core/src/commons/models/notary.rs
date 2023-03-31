use borsh::{BorshSerialize, BorshDeserialize};
use serde::{Serialize, Deserialize};

use crate::signature::Signature;

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
  PartialOrd
)]
pub struct NotaryEventResponse {
  pub notary_signature: Signature,
  pub gov_version_notary: u64,
}
