use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Debug, Clone, Eq, PartialEq, BorshSerialize, BorshDeserialize, ToSchema)]
pub struct TimeStamp {
    pub time: i128,
}

impl TimeStamp {
    pub fn now() -> Self {
        Self {
            time: OffsetDateTime::now_utc().unix_timestamp_nanos(),
        }
    }
}

// Serde compatible Serialize
impl Serialize for TimeStamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i128(self.time)
    }
}

impl<'de> Deserialize<'de> for TimeStamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let time = <i128 as Deserialize>::deserialize(deserializer)?;
        Ok(TimeStamp { time })
    }
}
