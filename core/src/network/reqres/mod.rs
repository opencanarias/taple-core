pub mod codec;

use instant::Duration;
use libp2p::request_response::{ProtocolSupport, RequestResponse, RequestResponseConfig};

use self::codec::{TapleCodec, TapleProtocol};

fn create_request_response_behaviour(response_interval: Duration) -> RequestResponse<TapleCodec> {
    let protocol = vec![(TapleProtocol { version: 1 }, ProtocolSupport::Full)];
    let codec = TapleCodec {};
    let mut config = RequestResponseConfig::default();
    config
        .set_connection_keep_alive(response_interval)
        .set_request_timeout(response_interval);

    RequestResponse::new(codec, protocol, config)
}
