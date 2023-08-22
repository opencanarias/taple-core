// Composed Behaviour for routing with kademlia and identify
use instant::Duration;
use libp2p::identify::Identify;
use libp2p::identify::IdentifyConfig;
use libp2p::identify::IdentifyEvent;
use libp2p::identity::Keypair;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{
    record::Key, AddProviderOk, Kademlia, KademliaBucketInserts, KademliaConfig, KademliaEvent,
    PeerRecord, PutRecordOk, QueryResult, Record,
};
use libp2p::kad::{QueryId, Quorum};
use libp2p::Multiaddr;
use libp2p::{NetworkBehaviour, PeerId};
use log::{debug, warn};

const LOG_TARGET: &str = "TAPLE_NETWORT::Routing";

// We create a custom network behaviour that combines Kademlia and Identify.
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "RoutingComposedEvent")]
pub struct RoutingBehaviour {
    kademlia: Kademlia<MemoryStore>,
    identify: Identify,
}

impl RoutingBehaviour {
    pub fn new(local_key: Keypair, bootstrap_nodes: Vec<(PeerId, Multiaddr)>) -> Self {
        let local_peer_id = PeerId::from(local_key.public());
        let config = KademliaConfig::default()
            .set_connection_idle_timeout(Duration::from_secs(600))
            .set_kbucket_inserts(KademliaBucketInserts::Manual)
            .to_owned();
        let store = MemoryStore::new(local_peer_id);
        let mut kademlia = Kademlia::with_config(local_peer_id, store, config);
        let identify = {
            let cfg = IdentifyConfig::new("/taple/1.0".to_string(), local_key.public())
                .with_agent_version("taple".to_owned());
            Identify::new(cfg)
        };
        // Add Bootstrap addresses to Kademlia routing table
        for (peer_id, addr) in &bootstrap_nodes {
            log::info!("ADDING; {:?} {:?}", peer_id, addr);
            kademlia.add_address(peer_id, addr.clone());
        }
        RoutingBehaviour { kademlia, identify }
    }

    /// Establishes the local node as the provider for that key in the given
    /// shard.
    pub fn start_providing<K>(&mut self, key: &K)
    where
        K: AsRef<[u8]>,
    {
        if let Err(e) = self.kademlia.start_providing(Key::new(&key)) {
            warn!("{}: Failed to start providing: {:?}", LOG_TARGET, e);
        }
    }

    pub fn handle_event(&mut self, event: RoutingComposedEvent) {
        match event {
            RoutingComposedEvent::IdentifyEvent(IdentifyEvent::Received { peer_id, info }) => {
                debug!(
                    "{}: ENTRANDO EN IDENTIFIED PARA {} con info: {:?}",
                    LOG_TARGET, peer_id, info
                );
                for addr in info.listen_addrs {
                    self.kademlia.add_address(&peer_id, addr);
                }
            }
            RoutingComposedEvent::IdentifyEvent(event) => {
                debug!("{}: Unhandled Identify Event: {:?}", LOG_TARGET, event);
            }
            RoutingComposedEvent::KademliaEvent(KademliaEvent::RoutingUpdated {
                peer,
                addresses,
                ..
            }) => {
                debug!(
                    "{}: Routing updated. Peer: {:?}; Addresses: {:?}.",
                    LOG_TARGET, peer, addresses,
                );
            }
            RoutingComposedEvent::KademliaEvent(KademliaEvent::PendingRoutablePeer {
                peer,
                address,
            }) => {
                debug!(
                    "{}: Pending Routable Peer: {:?}; Address: {:?}.",
                    LOG_TARGET, peer, address,
                );
            }
            RoutingComposedEvent::KademliaEvent(KademliaEvent::RoutablePeer {
                peer,
                address,
                ..
            }) => {
                debug!(
                    "{}: Routable Peer: {:?}; Address: {:?}.",
                    LOG_TARGET, peer, address,
                );
            }
            RoutingComposedEvent::KademliaEvent(KademliaEvent::UnroutablePeer { peer }) => {
                debug!("{}: Unroutable Peer: {:?}", LOG_TARGET, peer,);
            }
            RoutingComposedEvent::KademliaEvent(KademliaEvent::InboundRequest { request }) => {
                debug!("{}: Inbound Request: {:?}", LOG_TARGET, request);
            }
            RoutingComposedEvent::KademliaEvent(KademliaEvent::OutboundQueryCompleted {
                result,
                ..
            }) => match result {
                QueryResult::GetProviders(Ok(ok)) => {
                    for peer in ok.providers {
                        debug!("Peer {:?} provides key {:?}", peer, ok.key.as_ref());
                    }
                }
                QueryResult::GetProviders(Err(err)) => {
                    debug!("Failed to get providers: {:?}", err);
                }
                QueryResult::GetRecord(Ok(ok)) => {
                    for PeerRecord {
                        record: Record { key, value, .. },
                        ..
                    } in ok.records
                    {
                        debug!("Got record {:?} {:?}", key.as_ref(), &value,);
                    }
                }
                QueryResult::GetRecord(Err(err)) => {
                    debug!("Failed to get record: {:?}", err);
                }
                QueryResult::PutRecord(Ok(PutRecordOk { key })) => {
                    debug!("Successfully put record {:?}", key.as_ref());
                }
                QueryResult::PutRecord(Err(err)) => {
                    debug!("Failed to put record: {:?}", err);
                }
                QueryResult::StartProviding(Ok(AddProviderOk { key })) => {
                    debug!("Successfully put provider record {:?}", key.as_ref());
                }
                QueryResult::StartProviding(Err(err)) => {
                    debug!("Failed to put provider record: {:?}", err);
                }
                e => {
                    debug!("Unhandled QueryResult {:?}", e);
                }
            },
        }
    }

    pub fn bootstrap(&mut self) {
        let _ = self.kademlia.bootstrap();
    }

    #[allow(dead_code)]
    pub fn put_record(
        &mut self,
        record: Record,
        quorum: Quorum,
    ) -> Result<QueryId, libp2p::kad::record::store::Error> {
        self.kademlia.put_record(record, quorum)
    }

    #[allow(dead_code)]
    pub fn get_record(&mut self, key: Key, quorum: Quorum) -> QueryId {
        self.kademlia.get_record(key, quorum)
    }

    pub fn get_closest_peers(&mut self, peer_id: PeerId) -> QueryId {
        self.kademlia.get_closest_peers(peer_id)
    }
}

/// TAPLE network event
#[derive(Debug)]
pub enum RoutingComposedEvent {
    IdentifyEvent(IdentifyEvent),
    KademliaEvent(KademliaEvent),
}

/// Adapt `IdentifyEvent` to `RoutingComposedEvent`
impl From<IdentifyEvent> for RoutingComposedEvent {
    fn from(event: IdentifyEvent) -> RoutingComposedEvent {
        RoutingComposedEvent::IdentifyEvent(event)
    }
}

/// Adapt `KademliaEvent` to `RoutingComposedEvent`
impl From<KademliaEvent> for RoutingComposedEvent {
    fn from(event: KademliaEvent) -> RoutingComposedEvent {
        RoutingComposedEvent::KademliaEvent(event)
    }
}
