use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(non_snake_case)]
pub enum Quorum {
    #[serde(rename = "majority")]
    Majority,
    Fixed { fixed: u32 },
    Porcentaje { porcentaje: f64 },
    BFT { BFT: f64 },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Who {
    Id { id: String },
    #[serde(rename = "members")]
    Members,
    #[serde(rename = "all")]
    All,
    #[serde(rename = "external")]
    External,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Schema {
    Id { id: String },
    #[serde(rename = "all_schemas")]
    AllSchemas,
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
