use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize};
use std::hash::{Hash, Hasher};
use time::OffsetDateTime;

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, BorshSerialize, BorshDeserialize, Hash)]
pub struct TimeStamp {
    pub time: u64,
}

impl TimeStamp {
    pub fn now() -> Self {
        Self {
            time: OffsetDateTime::now_utc().unix_timestamp_nanos() as u64,
        }
    }
}

// Serde compatible Serialize
impl Serialize for TimeStamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.time)
    }
}

impl<'de> Deserialize<'de> for TimeStamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let time = <u64 as Deserialize>::deserialize(deserializer)?;
        Ok(TimeStamp { time })
    }
}
