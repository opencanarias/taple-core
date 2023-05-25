use std::collections::HashSet;

use serde::{de::Visitor, Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Quorum {
    Majority(String),
    #[serde(rename_all = "PascalCase")]
    Fixed { fixed: u32 },
    #[serde(rename_all = "PascalCase")]
    Porcentaje { porcentaje: f64 },
    #[serde(rename_all = "PascalCase")]
    BFT { BFT: f64 },
}

#[derive(Debug)]
pub enum Who {
    Id { id: String },
    Members,
    All,
    External,
}

impl Serialize for Who {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Who::Id { id } => serializer.serialize_str(&id),
            Who::Members => serializer.serialize_str("Members"),
            Who::All => serializer.serialize_str("All"),
            Who::External => serializer.serialize_str("External"),
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
                    "Members" => Ok(Who::Members),
                    "All" => Ok(Who::All),
                    "External" => Ok(Who::External),
                    &_ => Ok(Self::Value::Id { id: v }),
                }
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "Members" => Ok(Who::Members),
                    "All" => Ok(Who::All),
                    "External" => Ok(Who::External),
                    &_ => Ok(Self::Value::Id { id: v.to_string() }),
                }
            }
        }
        deserializer.deserialize_str(WhoVisitor {})
    }
}

#[derive(Debug)]
pub enum Schema {
    Id { id: String },
    AllSchemas,
}

impl Serialize for Schema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Schema::Id { id } => serializer.serialize_str(&id),
            Schema::AllSchemas => serializer.serialize_str("all_schemas"),
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
                    "all_schemas" => Ok(Self::Value::AllSchemas),
                    &_ => Ok(Self::Value::Id { id: v }),
                }
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "all_schemas" => Ok(Self::Value::AllSchemas),
                    &_ => Ok(Self::Value::Id { id: v.to_string() }),
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
    pub roles: HashSet<String>,
    pub schema: Schema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoke {
    pub fact: String,
    pub approval_required: bool,
    pub roles: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Facts {
    name: String,
    description: Option<String>,
    schema: serde_json::Value,
}
