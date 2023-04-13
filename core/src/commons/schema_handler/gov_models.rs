use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(non_snake_case)]
pub enum Quorum {
    Majority,
    Fixed { Fixed: u32 },
    Porcentaje { Porcentaje: f64 },
    BFT { BFT: f64 },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(non_snake_case)]
pub enum Who {
    IdObject { Id: String },
    Members,
    All,
    External,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Schema {
    IdObject { Id: String },
    AllSchemas,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Role {
    pub Who: Who,
    pub Namespace: String,
    pub Roles: HashSet<String>,
    pub Schema: Schema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoke {
    pub Fact: String,
    pub ApprovalRequired: bool,
    pub Roles: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    Name: String,
    Content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Facts {
    Name: String,
    Description: Option<String>,
    Schema: serde_json::Value,
}
