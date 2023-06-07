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

//! Defines the protocol handler and its prototype.
//!

use super::upgrade::TellProtocol;

use libp2p::swarm::{
    handler::{InboundUpgradeSend, OutboundUpgradeSend},
    ConnectionHandler, ConnectionHandlerEvent, ConnectionHandlerUpgrErr, KeepAlive,
    SubstreamProtocol,
};

use std::{
    collections::VecDeque,
    io,
    task::{Context, Poll},
    time::{Duration, Instant},
};

/// Defines struct for connection handler.
pub struct TellHandler {
    /// Max message size
    max_message_size: u64,
    /// Queue of events to emit in `poll()`.
    pending_events: VecDeque<TellHandlerEvent>,
    /// Outbound request pending
    outbound: VecDeque<TellProtocol>,
    /// A pending fatal error that results in the connection being closed.
    pending_error: Option<ConnectionHandlerUpgrErr<io::Error>>,
    /// Keep Alive
    keep_alive: KeepAlive,
    /// Substream KeepAlive
    subtream_timeout: Duration,
    /// Connection timeout
    connection_timeout: Duration,
}

impl TellHandler {
    pub fn new(max_message_size: u64, keep_alive: Duration, timeout: Duration) -> Self {
        Self {
            max_message_size,
            pending_events: VecDeque::new(),
            outbound: VecDeque::new(),
            pending_error: None,
            keep_alive: KeepAlive::Until(Instant::now() + keep_alive),
            subtream_timeout: timeout,
            connection_timeout: keep_alive,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TellHandlerEvent {
    /// An outbound tell timed out while waiting for the message
    OutboundTimeout,
    /// An inbound tell timed out while waiting for the message
    InboundTimeout,
    /// A request has been sent
    RequestSent,
    /// A request has arrived
    RequestReceived { data: Vec<u8> },
}

impl ConnectionHandler for TellHandler {
    type InEvent = TellProtocol;
    type OutEvent = TellHandlerEvent;
    type Error = ConnectionHandlerUpgrErr<io::Error>;
    type InboundProtocol = TellProtocol;
    type OutboundProtocol = TellProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        let proto = TellProtocol {
            message: vec![],
            max_message_size: self.max_message_size,
        };
        SubstreamProtocol::new(proto, ()).with_timeout(self.subtream_timeout)
    }

    /// Injects the output of a successful upgrade on a new inbound substream.
    fn inject_fully_negotiated_inbound(
        &mut self,
        protocol: <Self::InboundProtocol as InboundUpgradeSend>::Output,
        _info: Self::InboundOpenInfo,
    ) {
        self.pending_events
            .push_back(TellHandlerEvent::RequestReceived { data: protocol });
    }

    /// Injects the output of a successful upgrade on a new outbound substream.
    fn inject_fully_negotiated_outbound(
        &mut self,
        _protocol: <Self::OutboundProtocol as OutboundUpgradeSend>::Output,
        _info: Self::OutboundOpenInfo,
    ) {
        if self.outbound.is_empty() {
            self.keep_alive = KeepAlive::Until(Instant::now() + self.connection_timeout);
        }
        self.pending_events.push_back(TellHandlerEvent::RequestSent);
    }

    fn inject_event(&mut self, event: Self::InEvent) {
        self.keep_alive = KeepAlive::Yes;
        self.outbound.push_back(event);
    }

    /// Returns until when the connection should be kept alive.
    fn connection_keep_alive(&self) -> KeepAlive {
        self.keep_alive
    }

    /// Indicates to the handler that upgrading an outbound substream has
    /// failed.
    fn inject_dial_upgrade_error(
        &mut self,
        _: Self::OutboundOpenInfo,
        error: ConnectionHandlerUpgrErr<<Self::OutboundProtocol as OutboundUpgradeSend>::Error>,
    ) {
        self.keep_alive = KeepAlive::No;
        match error {
            ConnectionHandlerUpgrErr::Timeout => self
                .pending_events
                .push_back(TellHandlerEvent::OutboundTimeout),
            _ => {
                // Anything else is considered a fatal error or misbehaviour of
                // the remote peer and results in closing the connection.
                self.pending_error = Some(error);
            }
        }
    }

    fn inject_listen_upgrade_error(
        &mut self,
        _: Self::InboundOpenInfo,
        error: ConnectionHandlerUpgrErr<<Self::InboundProtocol as InboundUpgradeSend>::Error>,
    ) {
        self.keep_alive = KeepAlive::No;
        match error {
            ConnectionHandlerUpgrErr::Timeout => self
                .pending_events
                .push_back(TellHandlerEvent::InboundTimeout),
            _ => {
                self.pending_error = Some(error);
            }
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        if let Some(err) = self.pending_error.take() {
            return Poll::Ready(ConnectionHandlerEvent::Close(err));
        }

        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::Custom(event));
        }

        if let Some(proto) = self.outbound.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(proto, ()).with_timeout(self.subtream_timeout),
            });
        }

        Poll::Pending
    }
}
