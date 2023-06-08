use std::collections::HashMap;

use super::identifier::derive::{digest::DigestDerivator, KeyDerivator};
use config::Value;
use serde::Deserialize;

/// Configuration parameters of a TAPLE node divided into categories.
#[derive(Debug, Deserialize, Clone)]
pub struct TapleSettings {
    pub network: NetworkSettings,
    pub node: NodeSettings,
    pub database: DatabaseSettings,
}

/// P2P network configuration parameters of a TAPLE node.
#[derive(Debug, Deserialize, Clone)]
pub struct NetworkSettings {
    /// P2P Port
    #[serde(rename = "p2pport")]
    pub p2p_port: u32,
    /// [Multiaddr](https://github.com/multiformats/multiaddr) to consider by the node.
    pub addr: String,
    #[serde(rename = "knownnodes")]
    /// List of bootstrap nodes to connect to.
    pub known_nodes: Vec<String>,
    #[serde(rename = "externaladdress")]
    /// List of bootstrap nodes to connect to.
    pub external_address: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AccessPoint {
    #[serde(rename = "peer-id")]
    pub peer_id: String,
    pub addr: String,
}

/// General settings of a TAPLE node.
#[derive(Debug, Deserialize, Clone)]
pub struct NodeSettings {
    /// [KeyDerivator] to be used by the secret key.
    #[serde(rename = "keyderivator")]
    pub key_derivator: KeyDerivator,
    /// Secret key to be used by the node
    #[serde(rename = "secretkey")]
    pub secret_key: Option<String>,
    pub seed: Option<String>,
    /// [DigestDerivator] to be used for future event and subject identifiers
    #[serde(rename = "digestderivator")]
    pub digest_derivator: DigestDerivator,
    /// Percentage of network nodes receiving protocol messages in one iteration
    #[serde(rename = "replicationfactor")]
    pub replication_factor: f64,
    /// Timeout to be used between protocol iterations
    pub timeout: u32,
    /// Use Request-Response protocol to send messages throught the network
    pub req_res: bool,
    #[doc(hidden)]
    pub passvotation: u8,
    #[doc(hidden)]
    #[serde(rename = "devmode")]
    pub dev_mode: bool,
    pub smartcontracts_directory: String,
}

/// Configuration parameters of the database
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    /// Path where the database will be stored
    pub path: String,
}

impl From<AccessPoint> for Value {
    fn from(data: AccessPoint) -> Self {
        let mut map = HashMap::new();
        map.entry("peer_id".to_owned())
            .or_insert(Value::new(None, config::ValueKind::String(data.peer_id)));
        map.entry("addr".to_owned())
            .or_insert(Value::new(None, config::ValueKind::String(data.addr)));
        Self::new(None, config::ValueKind::Table(map))
    }
}

pub enum VotationType {
    Normal,
    AlwaysAccept,
    AlwaysReject,
}

impl From<u8> for VotationType {
    fn from(passvotation: u8) -> Self {
        match passvotation {
            2 => Self::AlwaysReject,
            1 => Self::AlwaysAccept,
            _ => Self::Normal,
        }
    }
}
