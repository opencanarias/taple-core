use taple_core::{Api, DigestIdentifier, ListenAddr, MemoryCollection, MemoryManager};

use taple_core::Node;

use super::error::{NotifierError, TapleError};
use super::notifier::TapleNotifier;

pub struct NodeBuilder {
    p2p_port: Option<u32>,
    access_points: Vec<String>,
    pass_votation: Option<u8>,
    secret_key: String,
}

#[allow(dead_code)]
impl NodeBuilder {
    pub fn new(private_key: String) -> Self {
        Self {
            p2p_port: None,
            access_points: Vec::new(),
            pass_votation: None,
            secret_key: private_key,
        }
    }

    pub fn build(self) -> TapleTestNode {
        let mut settings = Settings::default();
        settings.node.secret_key = Some(self.secret_key);
        settings.network.listen_addr = vec![ListenAddr::Memory {
            port: self.p2p_port,
        }];
        settings.network.known_nodes = self.access_points;
        settings.node.passvotation = self.pass_votation.unwrap_or(settings.node.passvotation);
        let path = format!("/tmp/.taple/sc");
        std::fs::create_dir_all(&path).expect("TMP DIR could not be created");
        settings.node.smartcontracts_directory = path;
        let database = MemoryManager::new();
        TapleTestNode::new(Node::new(settings, database))
    }

    pub fn with_port(mut self, port: u32) -> Self {
        self.p2p_port = Some(port);
        self
    }

    pub fn add_access_point(mut self, know_node: String) -> Self {
        self.access_points.push(know_node);
        self
    }

    pub fn pass_votation(mut self, pass_votation: PassVotation) -> Self {
        match pass_votation {
            PassVotation::AlwaysAccept => self.pass_votation = Some(1),
            PassVotation::AlwaysReject => self.pass_votation = Some(2),
        }
        self
    }
}

#[allow(dead_code)]
pub enum PassVotation {
    AlwaysAccept,
    AlwaysReject,
}

pub struct TapleTestNode {
    taple: Node<MemoryManager, MemoryCollection>,
    notifier: TapleNotifier,
    shutdown_manager: TapleShutdownManager,
}

impl TapleTestNode {
    pub fn new(taple: Node<MemoryManager, MemoryCollection>) -> Self {
        let notifier = taple.notification_handler();
        let shutdown_manager = taple.get_shutdown_manager();
        Self {
            taple,
            notifier: TapleNotifier::new(notifier),
            shutdown_manager,
        }
    }

    pub fn get_api(&self) -> Api {
        self.taple.api()
    }

    pub async fn start(&mut self) -> Result<(), TapleError> {
        self.taple
            .start()
            .await
            .map_err(|e| TapleError::StartError(e))
    }

    pub async fn shutdown(self) {
        self.shutdown_manager.shutdown().await;
    }

    pub async fn wait_for_new_subject(&mut self) -> Result<DigestIdentifier, NotifierError> {
        self.notifier.wait_for_new_subject().await
    }

    #[allow(dead_code)]
    pub async fn wait_for_new_event(&mut self) -> Result<(u64, DigestIdentifier), NotifierError> {
        self.notifier.wait_for_new_event().await
    }
}
