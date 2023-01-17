// Copyright 2022 Antonio Estevez <aestevez@opencanarias.es>

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at

// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing
// permissions and limitations under the License.

//! This module defines the protocol to pass a message to a node without
//! waiting for a response.

use std::{
    collections::{HashMap, VecDeque},
    task::Poll,
    time::Duration,
};

#[cfg(test)]
use std::collections::hash_map::Entry;

use libp2p::{
    core::{connection::ConnectionId, ConnectedPoint},
    swarm::{
        dial_opts::{self, DialOpts},
        IntoConnectionHandler, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler,
    },
    Multiaddr, PeerId,
};

mod handler;
mod upgrade;

use handler::{TellHandler, TellHandlerEvent};
use log::debug;

pub struct TellBehaviour {
    max_message_size: u64,
    pending_events: VecDeque<NetworkBehaviourAction<TellBehaviourEvent, TellHandler>>,
    connections: HashMap<PeerId, ConnectionId>,
    pending_outbonds: HashMap<PeerId, Vec<Vec<u8>>>,
    connection_keep_alive: Duration,
    substream_timeout: Duration,
    routes: HashMap<PeerId, libp2p::Multiaddr>,
}

impl TellBehaviour {
    pub fn new(max_message_size: u64, keep_alive: Duration, timeout: Duration) -> Self {
        Self {
            max_message_size,
            pending_events: VecDeque::new(),
            connections: HashMap::new(),
            pending_outbonds: HashMap::new(),
            connection_keep_alive: keep_alive,
            substream_timeout: timeout,
            routes: HashMap::new(),
        }
    }

    pub fn send_message(&mut self, peer: &PeerId, data: &[u8]) {
        if let Some(data) = self.try_send(peer, data) {
            let handler = self.new_handler();
            self.pending_events.push_back(NetworkBehaviourAction::Dial {
                opts: DialOpts::peer_id(*peer)
                    .condition(dial_opts::PeerCondition::NotDialing)
                    .build(),
                handler,
            });
            self.pending_outbonds.entry(*peer).or_default().push(data);
        }
    }

    fn try_send(&mut self, peer: &PeerId, data: &[u8]) -> Option<Vec<u8>> {
        if let Some(conn_id) = self.connections.get(peer) {
            let request = upgrade::TellProtocol {
                message: data.to_vec(),
                max_message_size: self.max_message_size,
            };
            self.pending_events
                .push_back(NetworkBehaviourAction::NotifyHandler {
                    peer_id: *peer,
                    handler: NotifyHandler::One(*conn_id),
                    event: request,
                });
            None
        } else {
            Some(data.to_vec())
        }
    }

    pub fn get_route(&self, peer_id: &PeerId) -> Option<&Multiaddr> {
        self.routes.get(peer_id)
    }

    pub fn set_route(&mut self, peer_id: PeerId, addr: Multiaddr) -> Option<Multiaddr> {
        self.routes.insert(peer_id, addr)
    }

    pub fn remove_route(&mut self, peer_id: &PeerId) -> Option<Multiaddr> {
        self.routes.remove(peer_id)
    }
}

pub enum RequestFailedError {
    ConnectionClosed,
}

#[derive(Debug)]
pub enum TellBehaviourEvent {
    RequestSent { peer_id: PeerId },
    RequestReceived { data: Vec<u8>, peer_id: PeerId },
    RequestFailed { peer_id: PeerId },
}

impl NetworkBehaviour for TellBehaviour {
    type ConnectionHandler = handler::TellHandler;
    type OutEvent = TellBehaviourEvent;

    fn addresses_of_peer(&mut self, peer: &PeerId) -> Vec<Multiaddr> {
        if let Some(addr) = self.routes.get(peer) {
            vec![addr.clone()]
        } else {
            vec![]
        }
    }

    fn inject_event(
        &mut self,
        peer_id: libp2p::PeerId,
        _connection: libp2p::core::connection::ConnectionId,
        event: TellHandlerEvent,
    ) {
        match event {
            TellHandlerEvent::InboundTimeout => {
                self.routes.remove(&peer_id);
                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        TellBehaviourEvent::RequestFailed { peer_id },
                    ))
            }
            TellHandlerEvent::OutboundTimeout => {
                self.routes.remove(&peer_id);
                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        TellBehaviourEvent::RequestFailed { peer_id },
                    ))
            }
            TellHandlerEvent::RequestReceived { data } => {
                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        TellBehaviourEvent::RequestReceived { data, peer_id },
                    ))
            }
            TellHandlerEvent::RequestSent => {
                self.pending_events
                    .push_back(NetworkBehaviourAction::GenerateEvent(
                        TellBehaviourEvent::RequestSent { peer_id },
                    ))
            }
        }
    }

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        Self::ConnectionHandler::new(
            self.max_message_size,
            self.connection_keep_alive,
            self.substream_timeout,
        )
    }

    fn inject_connection_closed(
        &mut self,
        peer_id: &PeerId,
        _conn: &ConnectionId,
        _: &ConnectedPoint,
        _: <Self::ConnectionHandler as IntoConnectionHandler>::Handler,
        _remaining_established: usize,
    ) {
        if self.connections.remove(peer_id).is_none() {
            debug!("Expected some established connection to peer before closing.");
        }
    }

    fn inject_connection_established(
        &mut self,
        peer_id: &PeerId,
        connection_id: &ConnectionId,
        _endpoint: &ConnectedPoint,
        _failed_addresses: Option<&Vec<libp2p::Multiaddr>>,
        _other_established: usize,
    ) {
        #[cfg(test)]
        {
            let addr = match _endpoint {
                ConnectedPoint::Dialer { address, .. } => address,
                ConnectedPoint::Listener { send_back_addr, .. } => send_back_addr,
            };
            match self.routes.entry(peer_id.clone()) {
                Entry::Occupied(mut data) => {
                    *data.get_mut() = addr.to_owned();
                }
                Entry::Vacant(data) => {
                    data.insert(addr.to_owned());
                }
            }
        }
        self.connections.entry(*peer_id).or_insert(*connection_id);

        if let Some(data) = self.pending_outbonds.remove(peer_id) {
            for request in data {
                self.try_send(peer_id, &request);
            }
        }
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
        _params: &mut impl libp2p::swarm::PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
        if let Some(ev) = self.pending_events.pop_front() {
            return Poll::Ready(ev);
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod test {
    use std::thread::sleep;

    use super::*;

    use futures::StreamExt;
    use tokio::runtime::Runtime;

    use libp2p::{
        core::{
            transport::{MemoryTransport, Transport},
            upgrade,
        },
        identity::Keypair,
        noise,
        swarm::{Swarm, SwarmEvent},
        yamux, Multiaddr,
    };

    #[test]
    fn test_tell() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut swarm1, _) = build_swarm(10);
            let (mut swarm2, addr2) = build_swarm(10);
            let remote_peer = swarm1.local_peer_id().clone();
            let payload = b"Hello worl!".to_vec();
            let payload2 = payload.clone();
            tokio::spawn(async move {
                loop {
                    match swarm2.select_next_some().await {
                        SwarmEvent::Behaviour(TellBehaviourEvent::RequestReceived {
                            data,
                            peer_id,
                        }) => {
                            assert_eq!(peer_id, remote_peer);
                            assert_eq!(data, payload2);
                            break;
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            assert_eq!(peer_id, remote_peer);
                        }
                        _ => {}
                    }
                }
            });
            swarm1.dial(addr2).unwrap();
            let mut request_received = false;
            loop {
                match swarm1.select_next_some().await {
                    SwarmEvent::Behaviour(TellBehaviourEvent::RequestSent { .. }) => {
                        request_received = true;
                    }
                    SwarmEvent::Behaviour(TellBehaviourEvent::RequestFailed { .. }) => {
                        assert!(false);
                        break;
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        swarm1.behaviour_mut().send_message(&peer_id, &payload);
                    }
                    SwarmEvent::ConnectionClosed { .. } => {
                        if request_received {
                            break;
                        }
                        assert!(false);
                    }
                    _ => {}
                }
            }
        });
    }

    #[test]
    fn test_tell_after_timeout() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut swarm1, _) = build_swarm(2);
            let (mut swarm2, addr2) = build_swarm(2);
            let remote_peer = swarm1.local_peer_id().clone();
            let payload = b"Hello worl!".to_vec();
            let payload2 = payload.clone();
            let id = tokio::spawn(async move {
                loop {
                    match swarm2.select_next_some().await {
                        SwarmEvent::Behaviour(TellBehaviourEvent::RequestReceived {
                            data,
                            peer_id,
                        }) => {
                            assert_eq!(peer_id, remote_peer);
                            assert_eq!(data, payload2);
                            break;
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            assert_eq!(peer_id, remote_peer);
                        }
                        _ => {}
                    }
                }
            });
            swarm1.dial(addr2.clone()).unwrap();
            let mut request_received = false;
            let mut has_closed = false;
            loop {
                match swarm1.select_next_some().await {
                    SwarmEvent::Behaviour(TellBehaviourEvent::RequestSent { .. }) => {
                        request_received = true;
                    }
                    SwarmEvent::Behaviour(TellBehaviourEvent::RequestFailed { .. }) => {
                        assert!(false);
                        break;
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        if !has_closed {
                            has_closed = true;
                            sleep(Duration::from_secs(2));
                            swarm1.behaviour_mut().send_message(&peer_id, &payload);
                        } else {
                            if request_received {
                                break;
                            }
                            assert!(false)
                        }
                    }
                    _ => {}
                }
            }
            id.await.unwrap();
        });
    }

    #[test]
    fn test_tell_multiple_req() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut swarm1, _) = build_swarm(2);
            let (mut swarm2, addr2) = build_swarm(2);
            let remote_peer = swarm1.local_peer_id().clone();
            let payload = b"Hello worl!".to_vec();
            let payload2 = payload.clone();
            let id = tokio::spawn(async move {
                let mut counter_received = 0;
                loop {
                    match swarm2.select_next_some().await {
                        SwarmEvent::Behaviour(TellBehaviourEvent::RequestReceived {
                            data,
                            peer_id,
                        }) => {
                            assert_eq!(peer_id, remote_peer);
                            assert_eq!(data, payload2);
                            counter_received += 1;
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            assert_eq!(peer_id, remote_peer);
                        }
                        SwarmEvent::ConnectionClosed { .. } => {
                            assert_eq!(counter_received, 10);
                            break;
                        }
                        SwarmEvent::Behaviour(TellBehaviourEvent::RequestFailed { .. }) => {
                            assert!(false);
                            break;
                        }
                        _ => {}
                    }
                }
            });
            swarm1.dial(addr2).unwrap();
            let mut counter_sent = 0;
            loop {
                match swarm1.select_next_some().await {
                    SwarmEvent::Behaviour(TellBehaviourEvent::RequestSent { .. }) => {
                        counter_sent += 1;
                    }
                    SwarmEvent::Behaviour(TellBehaviourEvent::RequestFailed { .. }) => {
                        assert!(false);
                        break;
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        for _ in 0..10 {
                            swarm1.behaviour_mut().send_message(&peer_id, &payload);
                        }
                    }
                    SwarmEvent::ConnectionClosed { .. } => {
                        assert_eq!(counter_sent, 10);
                        break;
                    }
                    _ => {}
                }
            }
            id.await.unwrap();
        });
    }

    fn build_swarm(seconds: u64) -> (Swarm<TellBehaviour>, Multiaddr) {
        let keypair = Keypair::generate_ed25519();

        let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&keypair)
            .unwrap();

        let transport = MemoryTransport
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
            .multiplex(yamux::YamuxConfig::default())
            .boxed();

        let behaviour = TellBehaviour::new(
            1000,
            Duration::from_secs(seconds),
            Duration::from_secs(seconds),
        );

        let mut swarm = Swarm::new(transport, behaviour, keypair.public().to_peer_id());
        let listen_addr: Multiaddr = format!("/memory/{}", rand::random::<u64>())
            .parse()
            .unwrap();

        swarm.listen_on(listen_addr.clone()).unwrap();

        (swarm, listen_addr)
    }
}
