pub mod codec;

use instant::Duration;
use libp2p::request_response::{ProtocolSupport, RequestResponse, RequestResponseConfig};

use self::codec::{TapleCodec, TapleProtocol};

fn create_request_response_behaviour(
    request_timeout: Duration,
    connection_keep_alive: Duration,
) -> RequestResponse<TapleCodec> {
    let protocol = vec![(TapleProtocol { version: 1 }, ProtocolSupport::Full)];
    let codec = TapleCodec {};
    let mut config = RequestResponseConfig::default();
    config
        .set_connection_keep_alive(connection_keep_alive)
        .set_request_timeout(request_timeout);

    RequestResponse::new(codec, protocol, config)
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
        request_response::{RequestId, RequestResponseEvent},
        swarm::{Swarm, SwarmEvent},
        yamux, Multiaddr,
    };

    #[test]
    fn test_req_res() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let (mut swarm1, _) = build_swarm(10);
            let (mut swarm2, addr2) = build_swarm(10);
            let remote_peer1 = swarm1.local_peer_id().clone();
            let remote_peer2 = swarm2.local_peer_id().clone();
            println!("RP1: {:?}", remote_peer1);
            println!("RP2: {:?}", remote_peer2);
            let payload = b"Hello!".to_vec();
            let payload2 = b"world!".to_vec();
            tokio::spawn(async move {
                loop {
                    match swarm2.select_next_some().await {
                        SwarmEvent::Behaviour(RequestResponseEvent::Message { peer, message }) => {
                            assert_eq!(peer, remote_peer1);
                            match message {
                                libp2p::request_response::RequestResponseMessage::Request {
                                    request_id,
                                    request,
                                    channel,
                                } => {
                                    println!("Request s2");
                                    assert_eq!(request, payload);
                                    swarm2
                                        .behaviour_mut()
                                        .send_response(channel, payload2.clone()).expect("va bien");
                                }
                                libp2p::request_response::RequestResponseMessage::Response {
                                    request_id,
                                    response,
                                } => panic!(),
                            }
                        }
                        SwarmEvent::Behaviour(RequestResponseEvent::ResponseSent {
                            peer,
                            request_id,
                        }) => {
                            println!("Response sent s2");
                            assert_eq!(peer, remote_peer1);
                        }
                        SwarmEvent::Behaviour(ev) => {
                            panic!("Unexpected event: {:?}", ev);
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            assert_eq!(peer_id, remote_peer1);
                        }
                        _ => {}
                    }
                }
            });
            swarm1.dial(addr2).unwrap();
            let mut response_received = false;
            let mut req_id: Option<RequestId> = None;
            loop {
                match swarm1.select_next_some().await {
                    SwarmEvent::Behaviour(RequestResponseEvent::Message { peer, message }) => {
                        response_received = true;
                        assert_eq!(peer, remote_peer2);
                        match message {
                            libp2p::request_response::RequestResponseMessage::Request {
                                request_id,
                                request,
                                channel,
                            } => {
                                panic!()
                            }
                            libp2p::request_response::RequestResponseMessage::Response {
                                request_id,
                                response,
                            } => {
                                println!("Response s1");
                                assert_eq!(response, b"world!".to_vec());
                                break;
                            }
                        }
                    }
                    SwarmEvent::Behaviour(ev) => {
                        panic!("Unexpected event: {:?}", ev);
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        println!("Request s1");
                        req_id = Some(
                            swarm1
                                .behaviour_mut()
                                .send_request(&peer_id, b"Hello!".to_vec()),
                        );
                    }
                    SwarmEvent::ConnectionClosed { .. } => {
                        sleep(Duration::from_secs(1));
                        if response_received {
                            break;
                        }
                        panic!()
                    }
                    _ => {}
                }
            }
        });
    }

    fn build_swarm(seconds: u64) -> (Swarm<RequestResponse<TapleCodec>>, Multiaddr) {
        let keypair = Keypair::generate_ed25519();

        let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&keypair)
            .unwrap();

        let transport = MemoryTransport::default()
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
            .multiplex(yamux::YamuxConfig::default())
            .boxed();

        let behaviour = create_request_response_behaviour(
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
