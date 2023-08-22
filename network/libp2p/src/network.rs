use super::{
    error::NetworkErrors,
    routing::{RoutingBehaviour, RoutingComposedEvent},
    tell::{TellBehaviour, TellBehaviourEvent},
};
use taple_core::{
    crypto::{KeyMaterial, KeyPair},
    Error as CoreError, ListenAddr,
};
use taple_core::{
    message::{Command, NetworkEvent},
    TapleNetwork,
};

use async_trait::async_trait;
use futures::StreamExt;
use instant::Duration;
use libp2p::{
    core::{
        either::EitherError,
        muxing::StreamMuxerBox,
        transport::{Boxed, MemoryTransport},
        upgrade,
    },
    dns,
    identity::{ed25519, Keypair},
    kad::{
        AddProviderOk, GetClosestPeersError, GetClosestPeersOk, KademliaEvent, PeerRecord,
        PutRecordOk, QueryResult,
    },
    mplex,
    multiaddr::Protocol,
    noise,
    swarm::{AddressScore, ConnectionHandlerUpgrErr, NetworkBehaviour, SwarmBuilder, SwarmEvent},
    tcp::TokioTcpConfig,
    yamux, Multiaddr, NetworkBehaviour, PeerId, Swarm, Transport,
};
use log::{debug, error, info};
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use tokio::sync::mpsc;

#[cfg(test)]
use libp2p::kad::{record::Key, QueryId, Quorum, Record};

const LOG_TARGET: &str = "TAPLE_NETWORT::Network";
const RETRY_TIMEOUT: u64 = 30000;

type TapleSwarmEvent = SwarmEvent<
    NetworkComposedEvent,
    EitherError<
        EitherError<std::io::Error, std::io::Error>,
        ConnectionHandlerUpgrErr<std::io::Error>,
    >,
>;

#[allow(dead_code)]
pub enum SendMode {
    RequestResponse,
    Tell,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "NetworkComposedEvent")]
pub struct TapleNetworkBehavior {
    routing: RoutingBehaviour,
    tell: TellBehaviour,
}

#[derive(Debug)]
pub enum NetworkComposedEvent {
    TellBehaviourEvent(TellBehaviourEvent),
    RoutingEvent(RoutingComposedEvent),
}

/// Adapt `IdentifyEvent` to `NetworkComposedEvent`
impl From<TellBehaviourEvent> for NetworkComposedEvent {
    fn from(event: TellBehaviourEvent) -> NetworkComposedEvent {
        NetworkComposedEvent::TellBehaviourEvent(event)
    }
}

/// Adapt `RoutingEvent` to `NetworkComposedEvent`
impl From<RoutingComposedEvent> for NetworkComposedEvent {
    fn from(event: RoutingComposedEvent) -> NetworkComposedEvent {
        NetworkComposedEvent::RoutingEvent(event)
    }
}

impl TapleNetworkBehavior {
    pub fn new(local_key: Keypair, bootstrap_nodes: Vec<(PeerId, Multiaddr)>) -> Self {
        let routing = RoutingBehaviour::new(local_key, bootstrap_nodes);
        let tell = TellBehaviour::new(100000, Duration::from_secs(10), Duration::from_secs(10));
        TapleNetworkBehavior { routing, tell }
    }

    #[cfg(test)]
    pub fn send_message(&mut self, peer: &PeerId, data: &[u8]) {
        self.tell.send_message(peer, data);
    }

    #[cfg(test)]
    pub fn bootstrap(&mut self) {
        self.routing.bootstrap();
    }

    #[cfg(test)]
    pub fn handle_rout_ev(&mut self, ev: RoutingComposedEvent) {
        self.routing.handle_event(ev);
    }

    #[allow(dead_code)]
    #[cfg(test)]
    pub fn put_record(
        &mut self,
        record: Record,
        quorum: Quorum,
    ) -> Result<QueryId, libp2p::kad::record::store::Error> {
        self.routing.put_record(record, quorum)
    }

    #[allow(dead_code)]
    #[cfg(test)]
    pub fn get_record(&mut self, key: Key, quorum: Quorum) -> QueryId {
        self.routing.get_record(key, quorum)
    }
}

fn check_listen_addr_integrity(addrs: &Vec<ListenAddr>) -> Result<ListenProtocols, NetworkErrors> {
    let mut has_memory = false;
    let mut has_ip = false;
    for addr in addrs {
        match addr {
            ListenAddr::Memory { .. } => has_memory = true,
            ListenAddr::IP4 { .. } => has_ip = true,
            ListenAddr::IP6 { .. } => has_ip = true,
        }
    }
    if has_memory && has_ip {
        return Err(NetworkErrors::ProtocolConflict);
    }
    if has_ip {
        return Ok(ListenProtocols::IP);
    } else {
        return Ok(ListenProtocols::Memory);
    }
}

/// Network Structure for connect message-sender, message-receiver and LibP2P network stack
pub struct NetworkProcessor {
    addr: Vec<ListenAddr>,
    swarm: Swarm<TapleNetworkBehavior>,
    command_sender: mpsc::Sender<Command>,
    command_receiver: mpsc::Receiver<Command>,
    // controller_mc: KeyPair,
    event_sender: mpsc::Sender<NetworkEvent>,
    pendings: HashMap<PeerId, VecDeque<Vec<u8>>>,
    // controller_to_peer: HashMap<Vec<u8>, PeerId>,
    // peer_to_controller: HashMap<PeerId, Vec<u8>>,
    active_get_querys: HashSet<PeerId>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
    pending_bootstrap_nodes: HashMap<PeerId, Multiaddr>,
    bootstrap_retries_steam:
        futures::stream::futures_unordered::FuturesUnordered<tokio::time::Sleep>,
    node_public_key: Vec<u8>,
    external_addresses: Vec<Multiaddr>,
}

enum ListenProtocols {
    Memory,
    IP,
}

impl NetworkProcessor {
    pub async fn new(
        addr: Vec<ListenAddr>,
        bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
        controller_mc: KeyPair,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        external_addresses: Vec<Multiaddr>,
    ) -> Result<Self, Box<dyn Error>> {
        let transport_protocol = check_listen_addr_integrity(&addr)?;
        let public_key = controller_mc.public_key_bytes();
        let local_key = {
            let sk = ed25519::SecretKey::from_bytes(controller_mc.secret_key_bytes())
                .expect("we always pass 32 bytes");
            Keypair::Ed25519(sk.into())
        };
        // Create a keypair for authenticated encryption of the transport.
        let noise_key: noise::AuthenticKeypair<noise::X25519Spec> =
            noise::Keypair::<noise::X25519Spec>::new()
                .into_authentic(&local_key)
                .expect("Signing libp2p-noise static DH keypair failed.");

        let transport = create_transport_by_protocol(transport_protocol, noise_key);
        let peer_id = local_key.public().to_peer_id();

        // Swarm creation
        let swarm = SwarmBuilder::new(
            transport,
            TapleNetworkBehavior::new(local_key, bootstrap_nodes.clone()),
            peer_id,
        )
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

        // Create channels to communicate events and commands
        let (command_sender, command_receiver) = mpsc::channel(10000);
        let pendings: HashMap<PeerId, VecDeque<Vec<u8>>> = HashMap::new();
        // let controller_to_peer: HashMap<Vec<u8>, PeerId> = HashMap::new();
        // let peer_to_controller: HashMap<PeerId, Vec<u8>> = HashMap::new();
        let active_get_querys: HashSet<PeerId> = HashSet::new();
        Ok(Self {
            node_public_key: public_key,
            addr,
            swarm,
            command_sender,
            command_receiver,
            event_sender: tokio::sync::mpsc::channel(1).0,
            // controller_mc,
            pendings,
            // controller_to_peer,
            // peer_to_controller,
            active_get_querys,
            shutdown_receiver,
            bootstrap_nodes,
            pending_bootstrap_nodes: HashMap::new(),
            bootstrap_retries_steam: futures::stream::futures_unordered::FuturesUnordered::new(),
            external_addresses,
        })
    }

    fn connect_to_pending_bootstraps(&mut self) {
        let keys: Vec<PeerId> = self.pending_bootstrap_nodes.keys().cloned().collect();
        for peer in keys {
            let addr = self.pending_bootstrap_nodes.remove(&peer).unwrap();
            let Ok(()) = self.swarm.dial(addr.to_owned()) else {
                panic!("Conection with bootstrap failed");
            };
        }
    }

    async fn handle_event(&mut self, event: TapleSwarmEvent) {
        match event {
            SwarmEvent::Dialing(peer_id) => {
                debug!("{}: Dialing to peer: {:?}", LOG_TARGET, peer_id);
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let local_peer_id = *self.swarm.local_peer_id();
                info!(
                    "listening on {:?}",
                    &address.with(Protocol::P2p(local_peer_id.into()))
                );
                // let addr_with_peer = address.clone().with(Protocol::P2p(local_peer_id.into()));
                // let addr_with_peer_bytes = addr_with_peer.to_vec();
                // let crypto_proof = self
                //     .controller_mc
                //     .sign(Payload::Buffer(addr_with_peer_bytes))
                //     .unwrap();
                // let value = bincode::serialize(&(addr_with_peer, crypto_proof)).unwrap();
                // match self.swarm.behaviour_mut().routing.put_record(
                //     Record {
                //         key: Key::new(&self.controller_mc.public_key_bytes()),
                //         value,
                //         publisher: None,
                //         expires: None,
                //     },
                //     Quorum::One,
                // ) {
                //     Ok(_) => (),
                //     Err(_) => panic!("HOLA"), // No debería fallar, ¿Tirarlo si falla?
                // }
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                debug!(
                    "{}: Connected to {} at {}",
                    LOG_TARGET,
                    peer_id,
                    endpoint.get_remote_address()
                );
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                cause: Some(error),
                ..
            } => {
                debug!(
                    "{}: Disconnected from {} with error {}",
                    LOG_TARGET, peer_id, error
                );
            }
            SwarmEvent::OutgoingConnectionError { error, peer_id } => {
                // Fixme for refused connections
                debug!("{}: Connection error: {}", LOG_TARGET, error);
                if let Some(peer_id) = peer_id {
                    // Delete cache peer id and address for that controller
                    self.swarm.behaviour_mut().tell.remove_route(&peer_id);
                    // Check if the peerID was a bootstrap node
                    if let Some((id, multiaddr)) =
                        self.bootstrap_nodes.iter().find(|(id, _)| *id == peer_id)
                    {
                        self.pending_bootstrap_nodes
                            .insert(*id, multiaddr.to_owned());
                        // Insert new timer if there was not any before
                        if self.bootstrap_retries_steam.len() == 0 {
                            self.bootstrap_retries_steam
                                .push(tokio::time::sleep(Duration::from_millis(RETRY_TIMEOUT)));
                        }
                    }
                    // match self.peer_to_controller.remove(&peer_id) {
                    //     Some(controller) => {
                    //         self.controller_to_peer.remove(&controller);
                    //     }
                    //     None => {}
                    // }
                }
            }
            SwarmEvent::Behaviour(behaviour_event) => match behaviour_event {
                NetworkComposedEvent::TellBehaviourEvent(ev) => match ev {
                    TellBehaviourEvent::RequestSent { peer_id } => {
                        debug!("{}: Request sent to: {}", LOG_TARGET, peer_id);
                        // TODO: Thinking about whether to delete here the pending list for a controller
                    }
                    TellBehaviourEvent::RequestReceived { data, peer_id } => {
                        debug!("{}: Request received from: {}", LOG_TARGET, peer_id);
                        self.event_sender
                            .send(NetworkEvent::MessageReceived { message: data })
                            .await
                            .expect("Event receiver not to be dropped.");
                    }
                    TellBehaviourEvent::RequestFailed { peer_id } => {
                        debug!("{}: Request failed to send to: {}", LOG_TARGET, peer_id);
                        // Delete cache peer id and address for that controller
                        // match self.peer_to_controller.remove(&peer_id) {
                        //     Some(controller) => {
                        //         self.controller_to_peer.remove(&controller);
                        //     }
                        //     None => {}
                        // }
                    }
                },

                NetworkComposedEvent::RoutingEvent(ev) => match ev {
                    RoutingComposedEvent::KademliaEvent(
                        KademliaEvent::OutboundQueryCompleted {
                            id: _,
                            result,
                            stats: _,
                        },
                    ) => match result {
                        QueryResult::GetRecord(Ok(ok)) => {
                            for PeerRecord { record, .. } in ok.records {
                                debug!("Got Record {:?}", record);
                            }
                            // for PeerRecord {
                            //     record:
                            //         Record {
                            //             key,
                            //             value,
                            //             publisher,
                            //             ..
                            //         },
                            //     ..
                            // } in ok.records
                            // {
                            //     let mc_bytes = key.to_vec();
                            //     let mc = Ed25519KeyPair::from_public_key(&mc_bytes);
                            //     match bincode::deserialize::<(Multiaddr, Vec<u8>)>(&value) {
                            //         Ok((addr, crypto_proof)) => {
                            //             // Comprobar la firma
                            //             match mc
                            //                 .verify(Payload::Buffer(addr.to_vec()), &crypto_proof)
                            //             {
                            //                 Ok(_) => {
                            //                     // Si está bien guardar en los hashmaps la info y hacer dial
                            //                     // Obtener el peerId a partir de la addr:
                            //                     let mut peer_id: Option<PeerId> = None;
                            //                     for protocol in addr.clone().iter() {
                            //                         if let Protocol::P2p(peer_id_multihash) =
                            //                             protocol
                            //                         {
                            //                             match PeerId::from_multihash(peer_id_multihash) {
                            //                                 Ok(pid) => {
                            //                                     if let Some(peer_id_publisher) = publisher {
                            //                                         // Comprobación de que el publisher es el propio peerId que buscamos
                            //                                         if pid != peer_id_publisher {
                            //                                             continue;
                            //                                         }
                            //                                     }
                            //                                     peer_id = Some(pid);
                            //                                     break;
                            //                                 },
                            //                                 Err(_) => debug!("Error al parsear multiaddr a peerId en get"),
                            //                             }
                            //                         }
                            //                     }
                            //                     if peer_id.is_none() {
                            //                         continue;
                            //                     }
                            //                     let peer_id = peer_id.unwrap();
                            //                     match self.swarm.dial(addr.clone()) {
                            //                         Ok(_) => {
                            //                             debug!("Success en DIAL");
                            //                             // Si funciona el dial actualizar las estructuras de datos
                            //                             self.routing_cache
                            //                                 .insert(peer_id, vec![addr]);
                            //                             // self.controller_to_peer
                            //                             //     .insert(mc_bytes.clone(), peer_id);
                            //                             // self.peer_to_controller
                            //                             //     .insert(peer_id, mc_bytes.clone());
                            //                             self.active_get_querys.remove(&peer_id);
                            //                             // Mandar mensajes pendientes
                            //                             self.send_pendings(&peer_id);
                            //                         }
                            //                         Err(e) => {
                            //                             debug!("{}", e);
                            //                             continue;
                            //                         }
                            //                     }
                            //                 }
                            //                 Err(_) => {
                            //                     continue;
                            //                 }
                            //             }
                            //         }
                            //         Err(e) => {
                            //             debug!("DESERIALICE VA MAL");
                            //             debug!("Problemas al recuperar la Multiaddr del value del Record: {:?}", e);
                            //         }
                            //     }
                            // }
                        }
                        QueryResult::GetRecord(Err(err)) => {
                            debug!("Failed to get record: {:?}", err);
                            // let mc_bytes = err.key().to_vec();
                            // self.active_get_querys.remove(&mc_bytes);
                        }
                        QueryResult::PutRecord(Ok(PutRecordOk { key })) => {
                            debug!("Successfully put record {:?}", key);
                        }
                        QueryResult::PutRecord(Err(err)) => {
                            debug!("Failed to put record: {:?}", err);
                        }
                        QueryResult::GetProviders(Ok(ok)) => {
                            for peer in ok.providers {
                                debug!("Peer {:?} provides key {:?}", peer, ok.key.as_ref());
                            }
                        }
                        QueryResult::GetProviders(Err(err)) => {
                            debug!("Failed to get providers: {:?}", err);
                        }
                        QueryResult::StartProviding(Ok(AddProviderOk { key })) => {
                            debug!("Successfully put provider record {:?}", key);
                        }
                        QueryResult::StartProviding(Err(err)) => {
                            debug!("Failed to put provider record: {:?}", err);
                        }
                        QueryResult::GetClosestPeers(gcp_res) => match gcp_res {
                            Ok(GetClosestPeersOk { key, .. }) => {
                                debug!("GCP OK: {:?}", key);
                                let peer_id = match PeerId::from_bytes(&key) {
                                    Ok(peer_id) => peer_id,
                                    Err(_) => {
                                        log::error!("Error parsing PeerId from GCP Ok response");
                                        return;
                                    }
                                };
                                self.active_get_querys.remove(&peer_id);
                            }
                            Err(GetClosestPeersError::Timeout { key, .. }) => {
                                debug!("GCP ERR: {:?}", key);
                                let peer_id = match PeerId::from_bytes(&key) {
                                    Ok(peer_id) => peer_id,
                                    Err(_) => {
                                        log::error!("Error parsing PeerId from GCP Err response");
                                        return;
                                    }
                                };
                                self.active_get_querys.remove(&peer_id);
                            }
                        },
                        e => {
                            debug!("Unhandled QueryResult {:?}", e);
                        }
                    },
                    RoutingComposedEvent::KademliaEvent(KademliaEvent::RoutablePeer {
                        peer,
                        address,
                    }) => {
                        debug!(
                            "{}: Routable Peer: {:?}; Address: {:?}.",
                            LOG_TARGET, peer, address,
                        );
                        if self.active_get_querys.contains(&peer) {
                            self.swarm.behaviour_mut().tell.set_route(peer, address);
                            self.active_get_querys.remove(&peer);
                            self.send_pendings(&peer);
                        }
                    }
                    _ => {
                        self.swarm.behaviour_mut().routing.handle_event(ev);
                    }
                },
            },
            other => {
                debug!("{}: Unhandled event {:?}", LOG_TARGET, other);
            }
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::StartProviding { keys } => {
                for key in keys {
                    self.swarm.behaviour_mut().routing.start_providing(&key);
                }
            }
            Command::SendMessage { receptor, message } => {
                // Check if we are the receptor
                if receptor == self.node_public_key {
                    // It is not needed to send the message
                    self.event_sender
                        .send(NetworkEvent::MessageReceived { message })
                        .await
                        .expect("Event receiver not to be dropped.");
                    return;
                }
                debug!("{}: Sending Message", LOG_TARGET);
                // Check if we have the peerId of the controller in cache
                let peer_id = match libp2p::identity::ed25519::PublicKey::decode(&receptor) {
                    Ok(public_key) => {
                        let public_key = libp2p::core::PublicKey::Ed25519(public_key);
                        PeerId::from_public_key(&public_key)
                    }
                    Err(_error) => {
                        log::error!(
                            "Error al tratar de enviar mensaje, el controllerId no es válido"
                        );
                        return;
                    }
                };

                // If we have it check if we have the address (need to fill in with cache addresses)
                let addresses_of_peer = self.swarm.behaviour_mut().addresses_of_peer(&peer_id);
                if !addresses_of_peer.is_empty() {
                    debug!("MANDANDO MENSAJE, TENGO DIRECCIÓN");
                    // If we have an address, send the message
                    self.swarm
                        .behaviour_mut()
                        .tell
                        .send_message(&peer_id, &message);
                    return;
                }

                // Check if we are not already making the same query in the DHT
                if let None = self.active_get_querys.get(&peer_id) {
                    // Make petition if we dont have it's PeerId to store it
                    self.active_get_querys.insert(peer_id.clone());
                    let query_id = self
                        .swarm
                        .behaviour_mut()
                        .routing
                        .get_closest_peers(peer_id.clone());
                    debug!(
                        "Query get_record {:?} para mandar request a {:?}",
                        query_id, peer_id
                    );
                }
                // Store de message in the pendings for that controller
                match self.pendings.get_mut(&peer_id) {
                    Some(pending_list) => {
                        if pending_list.len() >= 100 {
                            pending_list.pop_front();
                        }
                        pending_list.push_back(message);
                    }
                    None => {
                        let mut pendings = VecDeque::new();
                        pendings.push_back(message);
                        self.pendings.insert(peer_id, pendings);
                    }
                }
            }
            Command::Bootstrap => {
                self.swarm.behaviour_mut().routing.bootstrap();
            }
        }
    }

    /// Send all the pending messages to the specified controller
    fn send_pendings(&mut self, peer_id: &PeerId) {
        let pending_messages = self.pendings.remove(peer_id);
        if let Some(pending_messages) = pending_messages {
            for message in pending_messages.into_iter() {
                debug!("MANDANDO MENSAJE");
                self.swarm
                    .behaviour_mut()
                    .tell
                    .send_message(&peer_id, &message);
            }
        }
    }

    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }
}

#[async_trait]
impl TapleNetwork for NetworkProcessor {
    /// Network client
    fn client(&self) -> mpsc::Sender<Command> {
        self.command_sender.clone()
    }

    /// Run network processor.
    async fn run(&mut self, event_sender: mpsc::Sender<NetworkEvent>) {
        self.event_sender = event_sender;
        debug!("Running network");
        for external_address in self.external_addresses.clone().into_iter() {
            self.swarm
                .add_external_address(external_address, AddressScore::Infinite);
        }
        for addr in self.addr.iter() {
            if let Some(_) = addr.get_port() {
                let multiadd: Multiaddr = addr
                    .to_string()
                    .unwrap()
                    .parse()
                    .expect("String para multiaddress es válida");
                let result = self.swarm.listen_on(multiadd);
                if result.is_err() {
                    error!("Error: {:?}", result.unwrap_err());
                }
            }
        }
        for (_peer_id, addr) in self.bootstrap_nodes.iter() {
            let Ok(()) = self.swarm.dial(addr.to_owned()) else {
                    panic!("Conection with bootstrap failed");
                };
        }
        loop {
            tokio::select! {
                event = self.swarm.next() => self.handle_event(
                    event.expect("Swarm stream to be infinite.")).await,
                command = self.command_receiver.recv() => match command {
                    Some(c) => self.handle_command(c).await,
                    // Command channel closed, thus shutting down the network
                    // event loop.
                    None =>  {return;},
                },
                Some(_) = self.bootstrap_retries_steam.next() => self.connect_to_pending_bootstraps(),
                _ = self.shutdown_receiver.recv() => {
                    break;
                }
            }
        }
    }
}

fn create_ip4_ip6_transport(
    noise_key: noise::AuthenticKeypair<noise::X25519Spec>,
) -> Boxed<(PeerId, StreamMuxerBox)> {
    let transport = TokioTcpConfig::new()
        .nodelay(true)
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::NoiseConfig::xx(noise_key.clone()).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();
    // DNS
    match dns::GenDnsConfig::system(transport) {
        Ok(t) => t.boxed(),
        Err(_) => {
            // TODO: vuelvo a crear el transporte porque no tiene clone, quizás sería interesante poner una variable de entorno que diga si estamos en android y hacer lo segundo directamente en ese caso
            let transport = TokioTcpConfig::new()
                .nodelay(true)
                .upgrade(upgrade::Version::V1)
                .authenticate(noise::NoiseConfig::xx(noise_key.clone()).into_authenticated())
                .multiplex(mplex::MplexConfig::new())
                .boxed();
            // TODO: Lo mismo aquí
            match dns::GenDnsConfig::custom(
                transport,
                dns::ResolverConfig::cloudflare(),
                dns::ResolverOpts::default(),
            ) {
                Ok(t) => t.boxed(),
                Err(_) => TokioTcpConfig::new()
                    .nodelay(true)
                    .upgrade(upgrade::Version::V1)
                    .authenticate(noise::NoiseConfig::xx(noise_key.clone()).into_authenticated())
                    .multiplex(mplex::MplexConfig::new())
                    .boxed(),
            }
        }
    }
}

fn create_transport_by_protocol(
    protocol: ListenProtocols,
    noise_key: noise::AuthenticKeypair<noise::X25519Spec>,
) -> Boxed<(PeerId, StreamMuxerBox)> {
    match protocol {
        ListenProtocols::IP => create_ip4_ip6_transport(noise_key),
        ListenProtocols::Memory => MemoryTransport
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::NoiseConfig::xx(noise_key.clone()).into_authenticated())
            .multiplex(yamux::YamuxConfig::default())
            .boxed(),
    }
}

pub fn network_access_points(points: &[String]) -> Result<Vec<(PeerId, Multiaddr)>, CoreError> {
    let mut access_points: Vec<(PeerId, Multiaddr)> = Vec::new();
    for point in points {
        let data: Vec<&str> = point.split("/p2p/").collect();
        if data.len() != 2 {
            return Err(CoreError::AcessPointError(point.to_string()));
        }
        if let Some(value) = multiaddr(point) {
            if let Ok(id) = data[1].parse::<PeerId>() {
                access_points.push((id, value));
            } else {
                return Err(CoreError::AcessPointError(format!(
                    "Invalid PeerId conversion: {}",
                    point
                )));
            }
        } else {
            return Err(CoreError::AcessPointError(format!(
                "Invalid MultiAddress conversion: {}",
                point
            )));
        }
    }
    Ok(access_points)
}

pub fn external_addresses(addresses: &[String]) -> Result<Vec<Multiaddr>, CoreError> {
    let mut external_addresses: Vec<Multiaddr> = Vec::new();
    for address in addresses {
        if let Some(value) = multiaddr(address) {
            external_addresses.push(value);
        } else {
            return Err(CoreError::AcessPointError(format!(
                "Invalid MultiAddress conversion in External Address: {}",
                address
            )));
        }
    }
    Ok(external_addresses)
}

fn multiaddr(addr: &str) -> Option<Multiaddr> {
    match addr.parse::<Multiaddr>() {
        Ok(a) => Some(a),
        Err(_) => None,
    }
}
