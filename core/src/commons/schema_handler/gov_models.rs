use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(non_snake_case)]
pub enum Quorum {
    Majority,
    Fixed { Fixed: u64 },
    Porcentaje { Porcentaje: f64 },
    BFT { BFT: f64 },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(non_snake_case)]
pub enum Id {
    IdObject { Id: String },
    Members,
    All,
    External,
}
