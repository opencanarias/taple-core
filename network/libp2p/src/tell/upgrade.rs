/// Upgrade module for **tell** behaviour.
use libp2p::{
    core::{InboundUpgrade, OutboundUpgrade, UpgradeInfo},
    swarm::NegotiatedSubstream,
};

use futures::{future::BoxFuture, prelude::*};

use std::{io, iter};

/// Protocol for **tell** behaviour.
#[derive(Clone, Debug)]
pub struct TellProtocol {
    pub message: Vec<u8>,
    pub max_message_size: u64,
}

/// Implementation of protocol upgrade information. These upgrades can be
/// applied on inbound and outbound substreams.
impl UpgradeInfo for TellProtocol {
    type Info = &'static [u8];
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(b"/taple/tell/1.0.0")
    }
}

/// Upgrade on an outbound connection to perform the handshake with remote.
impl OutboundUpgrade<NegotiatedSubstream> for TellProtocol {
    type Output = ();
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    /// This method is called to start the handshake.
    fn upgrade_outbound(self, mut io: NegotiatedSubstream, _: Self::Info) -> Self::Future {
        async move {
            {
                let mut buffer = unsigned_varint::encode::usize_buffer();
                io.write_all(unsigned_varint::encode::usize(
                    self.message.len(),
                    &mut buffer,
                ))
                .await?;
            }
            io.write_all(&self.message).await?;
            io.close().await?;
            Ok(())
        }
        .boxed()
    }
}

/// Upgrade on an inbound connection to perform the handshake with remote.
impl InboundUpgrade<NegotiatedSubstream> for TellProtocol {
    type Output = Vec<u8>;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    /// This method is called to start the handshake.
    fn upgrade_inbound(self, mut io: NegotiatedSubstream, _: Self::Info) -> Self::Future {
        async move {
            // Read the length.
            let length = unsigned_varint::aio::read_usize(&mut io)
                .await
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            if length > usize::try_from(self.max_message_size).unwrap_or(usize::MAX) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "Message size exceeds limit: {} > {}",
                        length, self.max_message_size
                    ),
                ));
            }

            // Read the message.
            let mut buffer = vec![0; length];
            io.read_exact(&mut buffer).await?;
            Ok(buffer)
        }
        .boxed()
    }
}
