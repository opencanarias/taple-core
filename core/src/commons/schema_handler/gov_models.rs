use serde::{de::Visitor, ser::SerializeMap, Deserialize, Serialize};

#[derive(Debug)]
#[allow(non_snake_case)]
pub enum Quorum {
    MAJORITY,
    FIXED { fixed: u32 },
    PERCENTAGE { percentage: f64 },
    // BFT { BFT: f64 },
}

impl Serialize for Quorum {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Quorum::MAJORITY => serializer.serialize_str("MAJOTIRY"),
            Quorum::FIXED { fixed } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("FIXED", fixed)?;
                map.end()
            }
            Quorum::PERCENTAGE { percentage } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("PERCENTAGE", percentage)?;
                map.end()
            }
            // Quorum::BFT { BFT } => {
            //     let mut map = serializer.serialize_map(Some(1))?;
            //     map.serialize_entry("BFT", BFT)?;
            //     map.end()
            // }
        }
    }
}

impl<'de> Deserialize<'de> for Quorum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct QuorumEnumVisitor;
        impl<'de> Visitor<'de> for QuorumEnumVisitor {
            type Value = Quorum;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Quorum")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                // Solo deberían tener una entrada
                let Some(key) = map.next_key::<String>()? else {
                    return Err(serde::de::Error::missing_field("FIXED or PERCENTAGE"))
                };
                let result = match key.as_str() {
                    "FIXED" => {
                        let fixed: u32 = map.next_value()?;
                        Quorum::FIXED { fixed }
                    }
                    // "BFT" => {
                    //     let bft: f64 = map.next_value()?;
                    //     Quorum::BFT { BFT: bft }
                    // }
                    "PERCENTAGE" => {
                        let percentage: f64 = map.next_value()?;
                        Quorum::PERCENTAGE { percentage }
                    }
                    _ => {
                        return Err(serde::de::Error::unknown_field(
                            &key,
                            &["FIXED", "PERCENTAGE"],
                        ))
                    }
                };
                let None = map.next_key::<String>()? else {
                    return Err(serde::de::Error::custom("Input data is not valid. The data contains unkown entries"));
                };
                Ok(result)
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v.as_str() {
                    "MAJORITY" => Ok(Self::Value::MAJORITY),
                    other => Err(serde::de::Error::unknown_variant(other, &["MAJORITY"])),
                }
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "MAJORITY" => Ok(Self::Value::MAJORITY),
                    other => Err(serde::de::Error::unknown_variant(other, &["MAJORITY"])),
                }
            }
        }
        deserializer.deserialize_any(QuorumEnumVisitor {})
    }
}

#[derive(Debug)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub enum Who {
    ID { ID: String },
    NAME { NAME: String },
    MEMBERS,
    ALL,
    NOT_MEMBERS,
}

impl Serialize for Who {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Who::ID { ID } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("ID", ID)?;
                map.end()
            }
            Who::NAME { NAME } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("NAME", NAME)?;
                map.end()
            }
            Who::MEMBERS => serializer.serialize_str("MEMBERS"),
            Who::ALL => serializer.serialize_str("ALL"),
            Who::NOT_MEMBERS => serializer.serialize_str("NOT_MEMBERS"),
        }
    }
}

impl<'de> Deserialize<'de> for Who {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WhoVisitor;
        impl<'de> Visitor<'de> for WhoVisitor {
            type Value = Who;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Who")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                // Solo deberían tener una entrada
                let Some(key) = map.next_key::<String>()? else {
                    return Err(serde::de::Error::missing_field("ID or NAME"))
                };
                let result = match key.as_str() {
                    "ID" => {
                        let id: String = map.next_value()?;
                        Who::ID { ID: id }
                    }
                    "NAME" => {
                        let name: String = map.next_value()?;
                        Who::NAME { NAME: name }
                    }
                    _ => return Err(serde::de::Error::unknown_field(&key, &["ID", "NAME"])),
                };
                let None = map.next_key::<String>()? else {
                    return Err(serde::de::Error::custom("Input data is not valid. The data contains unkown entries"));
                };
                Ok(result)
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v.as_str() {
                    "MEMBERS" => Ok(Who::MEMBERS),
                    "ALL" => Ok(Who::ALL),
                    "NOT_MEMBERS" => Ok(Who::NOT_MEMBERS),
                    other => Err(serde::de::Error::unknown_variant(
                        other,
                        &["MEMBERS", "ALL", "NOT_MEMBERS"],
                    )),
                }
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "MEMBERS" => Ok(Who::MEMBERS),
                    "ALL" => Ok(Who::ALL),
                    "NOT_MEMBERS" => Ok(Who::NOT_MEMBERS),
                    other => Err(serde::de::Error::unknown_variant(
                        other,
                        &["MEMBERS", "ALL", "NOT_MEMBERS"],
                    )),
                }
            }
        }
        deserializer.deserialize_any(WhoVisitor {})
    }
}

#[derive(Debug)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub enum Schema {
    ID { ID: String },
    NOT_GOVERNANCE,
    ALL,
}

impl Serialize for Schema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Schema::ID { ID } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("ID", ID)?;
                map.end()
            }
            Schema::NOT_GOVERNANCE => serializer.serialize_str("NOT_GOVERNANCE"),
            Schema::ALL => serializer.serialize_str("ALL"),
        }
    }
}

impl<'de> Deserialize<'de> for Schema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SchemaEnumVisitor;
        impl<'de> Visitor<'de> for SchemaEnumVisitor {
            type Value = Schema;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Schema")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                // Solo deberían tener una entrada
                let Some(key) = map.next_key::<String>()? else {
                    return Err(serde::de::Error::missing_field("ID"))
                };
                let result = match key.as_str() {
                    "ID" => {
                        let id: String = map.next_value()?;
                        Schema::ID { ID: id }
                    }
                    _ => return Err(serde::de::Error::unknown_field(&key, &["ID", "NAME"])),
                };
                let None = map.next_key::<String>()? else {
                    return Err(serde::de::Error::custom("Input data is not valid. The data contains unkown entries"));
                };
                Ok(result)
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v.as_str() {
                    "ALL" => Ok(Self::Value::ALL),
                    "NOT_GOVERNANCE" => Ok(Self::Value::NOT_GOVERNANCE),
                    other => Err(serde::de::Error::unknown_variant(
                        other,
                        &["ALL", "NOT_GOVERNANCE"],
                    )),
                }
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "ALL" => Ok(Self::Value::ALL),
                    "NOT_GOVERNANCE" => Ok(Self::Value::NOT_GOVERNANCE),
                    other => Err(serde::de::Error::unknown_variant(
                        other,
                        &["ALL", "NOT_GOVERNANCE"],
                    )),
                }
            }
        }
        deserializer.deserialize_any(SchemaEnumVisitor {})
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Role {
    pub who: Who,
    pub namespace: String,
    pub role: String,
    pub schema: Schema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub raw: String,
}
