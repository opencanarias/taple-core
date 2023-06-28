use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize};
use std::hash::{Hash, Hasher};
use time::OffsetDateTime;

#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    BorshSerialize,
    BorshDeserialize,
    Hash,
    Serialize,
    Deserialize,
)]
pub struct TimeStamp(pub u64);

impl TimeStamp {
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc().unix_timestamp_nanos() as u64)
    }
}
