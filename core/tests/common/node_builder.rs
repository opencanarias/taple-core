use std::str::FromStr;

use taple_core::{
    Api, DigestIdentifier, Error, GoogleDns, ListenAddr, MemoryCollection, MemoryManager,
    Notification, Settings,
};

use taple_core::Node;
use tokio::time::{sleep, Duration};

use super::error::NotifierError;

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

    pub fn build(self) -> Result<OnMemoryNode, Error> {
        let mut settings = Settings::default();
        settings.node.secret_key = self.secret_key;
        settings.network.listen_addr = vec![ListenAddr::Memory {
            port: self.p2p_port,
        }];
        settings.network.known_nodes = self.access_points;
        settings.node.passvotation = self.pass_votation.unwrap_or(settings.node.passvotation);
        let path = format!("/tmp/.taple/sc");
        std::fs::create_dir_all(&path).expect("TMP DIR could not be created");
        settings.node.smartcontracts_directory = path;
        let database = MemoryManager::new();
        let google_dns = GoogleDns::new();
        let (node, api) = Node::build(settings, database, google_dns)?;
        Ok(OnMemoryNode::new(node, api))
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

pub struct OnMemoryNode {
    taple: Node<MemoryManager, MemoryCollection, GoogleDns>,
    api: Api,
}

const MAX_TIMEOUT_MS: u16 = 5000;

impl OnMemoryNode {
    pub fn new(taple: Node<MemoryManager, MemoryCollection, GoogleDns>, api: Api) -> Self {
        Self { taple, api }
    }

    pub fn get_api(&self) -> Api {
        self.api.clone()
    }

    pub async fn shutdown(self) {
        self.taple.shutdown_gracefully().await;
    }

    pub async fn wait_for_new_subject(&mut self) -> Result<DigestIdentifier, NotifierError> {
        let subject_id = self
            .wait_for_notification(|data| {
                if let Notification::NewSubject { subject_id } = data {
                    Some(subject_id)
                } else {
                    None
                }
            })
            .await?;
        Ok(DigestIdentifier::from_str(&subject_id)
            .expect("Invalid conversion to digest identifier"))
    }

    pub async fn wait_for_new_event(&mut self) -> Result<(u64, DigestIdentifier), NotifierError> {
        let (sn, subject_id) = self
            .wait_for_notification(|data| {
                if let Notification::NewEvent { sn, subject_id } = data {
                    Some((sn, subject_id))
                } else {
                    None
                }
            })
            .await?;
        Ok((
            sn,
            DigestIdentifier::from_str(&subject_id)
                .expect("Invalid conversion to digest identifier"),
        ))
    }

    async fn wait_for_notification<V, F: Fn(Notification) -> Option<V>>(
        &mut self,
        callback: F,
    ) -> Result<V, NotifierError> {
        loop {
            tokio::select! {
                _ = sleep(Duration::from_millis(MAX_TIMEOUT_MS as u64)) => {
                    return Err(NotifierError::RequestTimeout);
                },
                notification = self.taple.recv_notification() => {
                    match notification {
                        Some(data) => {
                            if let Some(result) = callback(data) {
                                return Ok(result);
                            }
                        },
                        None => {
                            break Err(NotifierError::NotificationChannelClosed);
                        }
                    }
                }
            }
        }
    }
}
