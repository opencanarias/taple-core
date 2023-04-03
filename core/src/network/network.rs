use super::{
    reqres::{codec::TapleCodec, create_request_response_behaviour},
    routing::{RoutingBehaviour, RoutingComposedEvent},
    tell::{TellBehaviour, TellBehaviourEvent},
};
use crate::commons::crypto::{KeyMaterial, KeyPair};
use crate::message::{Command, NetworkEvent};

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
    request_response::{RequestResponse, RequestResponseEvent, ResponseChannel},
    swarm::{ConnectionHandlerUpgrErr, NetworkBehaviour, SwarmBuilder, SwarmEvent},
    tcp::TokioTcpConfig,
    yamux, Multiaddr, NetworkBehaviour, PeerId, Swarm, Transport,
};
use log::{debug, info};
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
        EitherError<
            EitherError<std::io::Error, std::io::Error>,
            ConnectionHandlerUpgrErr<std::io::Error>,
        >,
        ConnectionHandlerUpgrErr<std::io::Error>,
    >,
>;

pub enum SendMode {
    RequestResponse,
    Tell,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "NetworkComposedEvent")]
pub struct TapleNetworkBehavior {
    routing: RoutingBehaviour,
    tell: TellBehaviour,
    req_res: RequestResponse<TapleCodec>,
}

#[derive(Debug)]
pub enum NetworkComposedEvent {
    TellBehaviourEvent(TellBehaviourEvent),
    RoutingEvent(RoutingComposedEvent),
    RequestResponseEvent(RequestResponseEvent<Vec<u8>, Vec<u8>>),
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

impl From<RequestResponseEvent<Vec<u8>, Vec<u8>>> for NetworkComposedEvent {
    fn from(event: RequestResponseEvent<Vec<u8>, Vec<u8>>) -> NetworkComposedEvent {
        NetworkComposedEvent::RequestResponseEvent(event)
    }
}

impl TapleNetworkBehavior {
    pub fn new(local_key: Keypair, bootstrap_nodes: Vec<(PeerId, Multiaddr)>) -> Self {
        let routing = RoutingBehaviour::new(local_key, bootstrap_nodes);
        let tell = TellBehaviour::new(10000, Duration::from_secs(10), Duration::from_secs(10));
        let req_res =
            create_request_response_behaviour(Duration::from_secs(10), Duration::from_secs(10));
        TapleNetworkBehavior {
            routing,
            tell,
            req_res,
        }
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

/// Network Structure for connect message-sender, message-receiver and LibP2P network stack
pub struct NetworkProcessor {
    addr: Multiaddr,
    swarm: Swarm<TapleNetworkBehavior>,
    command_sender: mpsc::Sender<Command>,
    command_receiver: mpsc::Receiver<Command>,
    event_sender: mpsc::Sender<NetworkEvent>,
    // controller_mc: KeyPair,
    pendings: HashMap<PeerId, VecDeque<Vec<u8>>>,
    // controller_to_peer: HashMap<Vec<u8>, PeerId>,
    // peer_to_controller: HashMap<PeerId, Vec<u8>>,
    active_get_querys: HashSet<PeerId>,
    shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
    bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
    pending_bootstrap_nodes: HashMap<PeerId, Multiaddr>,
    bootstrap_retries_steam:
        futures::stream::futures_unordered::FuturesUnordered<tokio::time::Sleep>,
    open_requests: HashMap<PeerId, VecDeque<ResponseChannel<Vec<u8>>>>,
    send_mode: SendMode,
}

impl NetworkProcessor {
    pub async fn new(
        addr: Option<String>,
        bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
        event_sender: mpsc::Sender<NetworkEvent>,
        controller_mc: KeyPair,
        shutdown_receiver: tokio::sync::broadcast::Receiver<()>,
        send_mode: SendMode,
    ) -> Result<Self, Box<dyn Error>> {
        let local_key = {
            let sk = ed25519::SecretKey::from_bytes(controller_mc.secret_key_bytes())
                .expect("we always pass 32 bytes");
            Keypair::Ed25519(sk.into())
        };
        // Create a keypair for authenticated encryption of the transport.
        let noise_key = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&local_key)
            .expect("Signing libp2p-noise static DH keypair failed.");

        let addr: Multiaddr = match addr {
            Some(add) => add.parse().expect("String para multiaddress es válida"),
            None => String::from("/ip4/0.0.0.0/tcp/0").parse().unwrap(),
        };

        let transport = {
            let mut transport: Option<Boxed<(PeerId, StreamMuxerBox)>> = None;
            for protocol in addr.clone().iter() {
                if let Protocol::Ip4(_) | Protocol::Ip6(_) = protocol {
                    transport = Some(
                        TokioTcpConfig::new()
                            .nodelay(true)
                            .upgrade(upgrade::Version::V1)
                            .authenticate(
                                noise::NoiseConfig::xx(noise_key.clone()).into_authenticated(),
                            )
                            .multiplex(mplex::MplexConfig::new())
                            .boxed(),
                    );
                    // DNS
                    transport = Some(
                        dns::GenDnsConfig::system(transport.unwrap())
                            // .await
                            .expect("DNS wont fail")
                            .boxed(),
                    );
                    break;
                } else if let Protocol::Memory(_) = protocol {
                    transport = Some(
                        MemoryTransport
                            .upgrade(upgrade::Version::V1)
                            .authenticate(
                                noise::NoiseConfig::xx(noise_key.clone()).into_authenticated(),
                            )
                            .multiplex(yamux::YamuxConfig::default())
                            .boxed(),
                    );
                    break;
                }
            }
            transport.expect(
                "String for address in constructor is valid and has ipv4 or memory protocol",
            )
        };
        let peer_id = local_key.public().to_peer_id();

        // DNS
        let transport = dns::GenDnsConfig::system(transport)?.boxed();

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
            addr,
            swarm,
            command_sender,
            command_receiver,
            event_sender,
            // controller_mc,
            pendings,
            // controller_to_peer,
            // peer_to_controller,
            active_get_querys,
            shutdown_receiver,
            bootstrap_nodes,
            pending_bootstrap_nodes: HashMap::new(),
            bootstrap_retries_steam: futures::stream::futures_unordered::FuturesUnordered::new(),
            open_requests: HashMap::new(),
            send_mode,
        })
    }

    /// Network client
    pub fn client(&self) -> mpsc::Sender<Command> {
        self.command_sender.clone()
    }

    /// Run network processor.
    pub async fn run(mut self) {
        debug!("Running network");
        let a = self.swarm.listen_on(self.addr.clone());
        if a.is_err() {
            println!("Error: {:?}", a.unwrap_err());
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
                    "RED: {:?}",
                    address.clone().with(Protocol::P2p(local_peer_id.into()))
                );
                debug!(
                    "{}: Local node is listening on {:?}",
                    LOG_TARGET,
                    address.clone().with(Protocol::P2p(local_peer_id.into()))
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
                NetworkComposedEvent::RequestResponseEvent(req_res_event) => match req_res_event {
                    RequestResponseEvent::Message { peer, message } => match message {
                        libp2p::request_response::RequestResponseMessage::Request {
                            request,
                            channel,
                            ..
                        } => {
                            log::debug!("Request received from peer: {:?}", peer);
                            // Save Response Channel
                            self.open_requests
                                .entry(peer)
                                .or_insert(VecDeque::new())
                                .push_back(channel);
                            // Pass message to MessageReceiver
                            self.event_sender
                                .send(NetworkEvent::MessageReceived { message: request })
                                .await
                                .expect("Event receiver not to be dropped.");
                        }
                        libp2p::request_response::RequestResponseMessage::Response {
                            response,
                            ..
                        } => {
                            log::debug!("Response received from peer: {:?}", peer);
                            // Pass message to MessageReceiver
                            self.event_sender
                                .send(NetworkEvent::MessageReceived { message: response })
                                .await
                                .expect("Event receiver not to be dropped.");
                        }
                    },
                    RequestResponseEvent::OutboundFailure { peer, error, .. } => {
                        log::error!(
                            "OutboundFailure in request response: {:?} to peer: {:?}",
                            error,
                            peer
                        );
                    }
                    RequestResponseEvent::InboundFailure { peer, error, .. } => {
                        log::error!(
                            "InboundFailure in request response: {:?} to peer: {:?}",
                            error,
                            peer
                        );
                    }
                    RequestResponseEvent::ResponseSent { peer, .. } => {
                        log::debug!("Response sent to peer: {:?}", peer);
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

                // Check if we have request open with peer, so we have to send the message as a response
                if let Some(requests) = self.open_requests.get_mut(&peer_id) {
                    while let Some(channel) = requests.pop_front() {
                        if channel.is_open() {
                            debug!(
                                "{}: Sending Message as Response to {:?}",
                                LOG_TARGET, peer_id
                            );
                            if let Err(error) = self
                                .swarm
                                .behaviour_mut()
                                .req_res
                                .send_response(channel, message.clone())
                            {
                                log::error!(
                                    "Error sending response: {:?}, to {:?}",
                                    error,
                                    peer_id
                                );
                            } else {
                                return;
                            }
                        }
                    }
                }

                // If we have it check if we have the address (falta rellenar con direcciones de la cache)
                let addresses_of_peer = self.swarm.behaviour_mut().addresses_of_peer(&peer_id);
                if !addresses_of_peer.is_empty() {
                    debug!("MANDANDO MENSAJE, TENGO DIRECCIÓN");
                    // If we have an address, send the message
                    match self.send_mode {
                        SendMode::RequestResponse => {
                            let _req_id = self
                                .swarm
                                .behaviour_mut()
                                .req_res
                                .send_request(&peer_id, message);
                        }
                        SendMode::Tell => {
                            self.swarm
                                .behaviour_mut()
                                .tell
                                .send_message(&peer_id, &message);
                        }
                    }
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
                // Store de message en the pendings for that controller
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
                match self.send_mode {
                    SendMode::RequestResponse => {
                        let _req_id = self
                            .swarm
                            .behaviour_mut()
                            .req_res
                            .send_request(&peer_id, message);
                    }
                    SendMode::Tell => {
                        self.swarm
                            .behaviour_mut()
                            .tell
                            .send_message(&peer_id, &message);
                    }
                }
            }
        }
    }

    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }
}
