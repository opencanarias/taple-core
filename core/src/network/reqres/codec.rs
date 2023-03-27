use async_trait::async_trait;
use futures::prelude::*;
use libp2p::{
    core::upgrade::{read_length_prefixed, write_length_prefixed, ProtocolName},
    request_response::RequestResponseCodec,
};
use std::io;

#[derive(Clone)]
pub struct TapleProtocol {
    version: u32,
}

impl ProtocolName for TapleProtocol {
    fn protocol_name(&self) -> &[u8] {
        match self.version {
            1 => b"/taple/1.0.0",
            _ => panic!("Unsupported TapleProtocol version"),
        }
    }
}

pub struct TapleCodec {}

#[async_trait]
impl RequestResponseCodec for TapleCodec {
    type Protocol = TapleProtocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    async fn read_request<T>(&mut self, _: &TapleProtocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 1_000_000).await?;

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(vec)
    }

    async fn read_response<T>(
        &mut self,
        _: &TapleProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 1_000_000).await?;

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(vec)
    }

    async fn write_request<T>(
        &mut self,
        _: &TapleProtocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, &req).await?;
        io.close().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &TapleProtocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, &res).await?;
        io.close().await?;

        Ok(())
    }
}
