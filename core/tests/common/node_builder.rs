use taple_core::{
    identifier::derive::{digest::DigestDerivator, KeyDerivator},
    DatabaseSettings, MemoryManager, NetworkSettings, NodeSettings, TapleSettings,
};

use taple_core::Taple;

pub struct NodeBuilder {
    timeout: Option<u32>,
    replication_factor: Option<f64>,
    digest_derivator: Option<DigestDerivator>,
    key_derivator: Option<KeyDerivator>,
    database_path: Option<String>,
    p2p_port: Option<u32>,
    addr: Option<String>,
    access_points: Option<Vec<String>>,
    seed: Option<String>,
    pass_votation: Option<u32>,
    dev_mode: Option<bool>,
    secret_key: Option<String>,
}

impl NodeBuilder {
    pub fn new() -> Self {
        Self {
            timeout: None,
            replication_factor: None,
            digest_derivator: None,
            key_derivator: None,
            database_path: None,
            p2p_port: None,
            addr: None,
            access_points: None,
            seed: None,
            pass_votation: None,
            dev_mode: None,
            secret_key: None,
        }
    }

    pub fn build(mut self) -> Taple<MemoryManager> {
        let settings = TapleSettings {
            network: NetworkSettings {
                p2p_port: self.p2p_port.unwrap_or(40000u32),
                addr: self.addr.unwrap_or("/ip4/127.0.0.1/tcp".into()),
                known_nodes: self.access_points.unwrap_or(vec![]),
            },
            node: NodeSettings {
                key_derivator: self.key_derivator.take().unwrap_or(KeyDerivator::Ed25519),
                secret_key: self.secret_key,
                seed: self.seed,
                digest_derivator: self
                    .digest_derivator
                    .take()
                    .unwrap_or(DigestDerivator::Blake3_256),
                replication_factor: self.replication_factor.take().unwrap_or(25f64),
                timeout: self.timeout.take().unwrap_or(3000u32),
                passvotation: self.pass_votation.unwrap_or(0) as u8,
                dev_mode: self.dev_mode.take().unwrap_or(false),
                smartcontracts_directory: "../../../contracts".into(),
            },
            database: DatabaseSettings {
                path: self.database_path.unwrap_or("".into()),
            },
        };
        Taple::new(settings, MemoryManager::new())
    }

    #[allow(dead_code)]
    pub fn with_database_path(mut self, path: String) -> Self {
        self.database_path = Some(path);
        self
    }

    #[allow(dead_code)]
    pub fn add_access_point(mut self, access_point: String) -> Self {
        if self.access_points.is_none() {
            self.access_points = Some(Vec::new());
        }
        let access_points = self.access_points.as_mut().unwrap();
        access_points.push(access_point);
        self
    }

    pub fn with_p2p_port(mut self, port: u32) -> Self {
        self.p2p_port = Some(port);
        self
    }

    pub fn with_addr(mut self, addr: String) -> Self {
        self.addr = Some(addr);
        self
    }

    #[allow(dead_code)]
    pub fn with_seed(mut self, seed: String) -> Self {
        self.seed = Some(seed);
        self
    }

    #[allow(dead_code)]
    pub fn with_secret_key(mut self, sk: String) -> Self {
        self.secret_key = Some(sk);
        self
    }

    #[allow(dead_code)]
    pub fn with_timeout(mut self, timeout: u32) -> Self {
        self.timeout = Some(timeout);
        self
    }

    #[allow(dead_code)]
    pub fn with_pass_votation(mut self, pass_votation: u32) -> Self {
        self.pass_votation = Some(pass_votation);
        self
    }

    #[allow(dead_code)]
    pub fn with_replication_factor(mut self, replication_factor: f64) -> Self {
        self.replication_factor = Some(replication_factor);
        self
    }

    #[allow(dead_code)]
    pub fn with_digest_derivator(mut self, derivator: DigestDerivator) -> Self {
        self.digest_derivator = Some(derivator);
        self
    }

    #[allow(dead_code)]
    pub fn with_dev_mode(mut self, mode: bool) -> Self {
        self.dev_mode = Some(mode);
        self
    }

    #[allow(dead_code)]
    pub fn with_key_derivator(mut self, derivator: KeyDerivator) -> Self {
        self.key_derivator = Some(derivator);
        self
    }
}
