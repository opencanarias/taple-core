use std::collections::HashSet;

use serde::{de::Visitor, Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(non_snake_case)]
pub enum Quorum {
    MAJORITY(String),
    #[serde(rename_all = "PascalCase")]
    FIXED {
        fixed: u32,
    },
    #[serde(rename_all = "PascalCase")]
    PORCENTAJE {
        porcentaje: f64,
    },
    #[serde(rename_all = "PascalCase")]
    BFT {
        BFT: f64,
    },
}

#[derive(Debug)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub enum Who {
    ID { ID: String },
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
            Who::ID { ID } => serializer.serialize_str(&ID),
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
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Who")
            }
            type Value = Who;
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v.as_str() {
                    "MEMBERS" => Ok(Who::MEMBERS),
                    "All" => Ok(Who::ALL),
                    "NOT_MEMBERS" => Ok(Who::NOT_MEMBERS),
                    &_ => Ok(Self::Value::ID { ID: v }),
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
                    &_ => Ok(Self::Value::ID { ID: v.to_string() }),
                }
            }
        }
        deserializer.deserialize_str(WhoVisitor {})
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
            Schema::ID { ID } => serializer.serialize_str(&ID),
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
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Schema")
            }
            type Value = Schema;
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v.as_str() {
                    "ALL" => Ok(Self::Value::ALL),
                    "NOT_GOVERNANCE" => Ok(Self::Value::NOT_GOVERNANCE),
                    &_ => Ok(Self::Value::ID { ID: v }),
                }
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "ALL" => Ok(Self::Value::ALL),
                    "NOT_GOVERNANCE" => Ok(Self::Value::NOT_GOVERNANCE),
                    &_ => Ok(Self::Value::ID { ID: v.to_string() }),
                }
            }
        }
        deserializer.deserialize_str(SchemaEnumVisitor {})
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
    pub name: String,
    pub content: String,
}
