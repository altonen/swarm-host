use crate::{
    backend::{NetworkBackend, WithMessageInfo},
    error::Error,
};

use futures::SinkExt;
use serde::Serialize;
use tokio::{net::TcpListener, sync::mpsc};
use tokio_tungstenite::tungstenite::Message;

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::{Duration, Instant},
};

/// Logging target for the file.
const LOG_TARGET: &str = "heuristics";

/// Update heuristics front-end every 3 seconds.
const UPDATE_INTERVAL: u64 = 3u64;

/// Events sent by the [`HeuristicsHandle`] to [`HeuristicsBackend`].
#[derive(Debug, Clone)]
enum HeuristicsEvent<T: NetworkBackend> {
    /// Register interface.
    _RegisterInterface {
        /// Interface ID.
        interface: T::InterfaceId,
    },

    /// Link interfaces.
    LinkInterfaces {
        /// ID of the first interface.
        _first: T::InterfaceId,

        /// ID of the second interface.
        _second: T::InterfaceId,
    },

    /// Unlink interfaces.
    _UnlinkInterfaces {
        /// ID of the first interface.
        first: T::InterfaceId,

        /// ID of the second interface.
        second: T::InterfaceId,
    },

    /// Register peer.
    RegisterPeer {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Peer ID.
        peer: T::PeerId,
    },

    /// Unregister interface
    UnregisterPeer {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Peer ID.
        peer: T::PeerId,
    },

    /// Message was received from `peer` to `interface`.
    MessageReceived {
        /// Interface ID.
        _interface: T::InterfaceId,

        /// Protocol.
        protocol: T::Protocol,

        /// Peer ID.
        peer: T::PeerId,

        /// Notification hash.
        hash: u64,

        /// Notification size.
        size: usize,
    },

    /// Message was sent from `interface` to `peers`.
    MessageSent {
        /// Interface ID.
        _interface: T::InterfaceId,

        /// Protocol.
        protocol: T::Protocol,

        /// Peer IDs.
        peers: Vec<T::PeerId>,

        /// Notification hash.
        hash: u64,

        /// Notification size.
        size: usize,
    },
}

/// Handle to registers events to [`HeuristicsBackend`].
#[derive(Debug, Clone)]
pub struct HeuristicsHandle<T: NetworkBackend> {
    /// TX channel for sending events to [`HeuristicsBackend`].
    tx: mpsc::UnboundedSender<HeuristicsEvent<T>>,
}

impl<T: NetworkBackend> HeuristicsHandle<T> {
    /// Register interface.
    pub fn _register_interface(&self, interface: T::InterfaceId) {
        self.tx
            .send(HeuristicsEvent::_RegisterInterface { interface })
            .expect("channel to stay open");
    }

    /// Link interface.
    pub fn link_interfaces(&self, _first: T::InterfaceId, _second: T::InterfaceId) {
        self.tx
            .send(HeuristicsEvent::LinkInterfaces { _first, _second })
            .expect("channel to stay open");
    }

    /// Unlink interface.
    pub fn _unlink_interfaces(&self, first: T::InterfaceId, second: T::InterfaceId) {
        self.tx
            .send(HeuristicsEvent::_UnlinkInterfaces { first, second })
            .expect("channel to stay open");
    }

    /// Register `peer` to `interface`'s known peers.
    pub fn register_peer(&self, interface: T::InterfaceId, peer: T::PeerId) {
        self.tx
            .send(HeuristicsEvent::RegisterPeer { interface, peer })
            .expect("channel to stay open");
    }

    /// Unregister `peer` from `interface`'s known peers.
    pub fn unregister_peer(&self, interface: T::InterfaceId, peer: T::PeerId) {
        self.tx
            .send(HeuristicsEvent::UnregisterPeer { interface, peer })
            .expect("channel to stay open");
    }

    /// Register that `notification` was received from `peer` to `interface`.
    pub fn register_notification_received(
        &self,
        _interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        notification: &T::Message,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageReceived {
                _interface,
                protocol,
                peer,
                hash: notification.hash(),
                size: notification.size(),
            })
            .expect("channel to stay open");
    }

    /// Register that `notification` was forwarded from `interface` to `peers`.
    pub fn register_notification_sent(
        &self,
        _interface: T::InterfaceId,
        protocol: T::Protocol,
        peers: Vec<T::PeerId>,
        notification: &T::Message,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageSent {
                _interface,
                protocol,
                peers,
                hash: notification.hash(),
                size: notification.size(),
            })
            .expect("channel to stay open");
    }

    /// Register that `request` was received from `peer` to `interface.`
    pub fn register_request_received(
        &self,
        _interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        request: &T::Request,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageReceived {
                _interface,
                protocol,
                peer,
                hash: request.hash(),
                size: request.size(),
            })
            .expect("channel to stay open");
    }

    /// Register that `request` was sent to `peer` from `interface.`
    pub fn _register_request_sent(
        &self,
        _interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        request: &T::Request,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageSent {
                _interface,
                protocol,
                peers: vec![peer],
                hash: request.hash(),
                size: request.size(),
            })
            .expect("channel to stay open");
        todo!();
    }

    /// Register that `response` was received from `peer` to `interface.`
    pub fn register_response_received(
        &self,
        _interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        response: &T::Response,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageReceived {
                _interface,
                protocol,
                peer,
                hash: response.hash(),
                size: response.size(),
            })
            .expect("channel to stay open");
    }

    /// Register that `response` was sent to `peer` from `interface.`
    pub fn _register_response_sent(
        &self,
        _interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        response: &T::Response,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageSent {
                _interface,
                protocol,
                peers: vec![peer],
                hash: response.hash(),
                size: response.size(),
            })
            .expect("channel to stay open");
    }
}

#[derive(Debug, Serialize, Default)]
struct MessageHeuristics {
    total_bytes_sent: usize,
    total_messages_sent: usize,
    total_bytes_received: usize,
    total_messages_received: usize,
    redundant_bytes_sent: usize,
    redundant_bytes_received: usize,
    unique_messages_sent: HashSet<u64>,
    unique_messages_received: HashSet<u64>,
}

#[derive(Debug, Serialize)]
struct PeerHeuristics<T: NetworkBackend> {
    /// Interfaces connected to this peer.
    interfaces: HashSet<T::InterfaceId>,

    /// Message heuristics.
    protocols: HashMap<T::Protocol, MessageHeuristics>,
}

impl<T: NetworkBackend> Default for PeerHeuristics<T> {
    fn default() -> Self {
        Self {
            interfaces: Default::default(),
            protocols: Default::default(),
        }
    }
}

impl<T: NetworkBackend> PeerHeuristics<T> {
    pub fn register_message_received(&mut self, protocol: &T::Protocol, hash: u64, size: usize) {
        let mut entry = self.protocols.entry(protocol.to_owned()).or_default();

        entry.total_bytes_received += size;
        entry.total_messages_received += 1;

        if !entry.unique_messages_received.insert(hash) {
            entry.redundant_bytes_received += size;
        }
    }

    /// Register that a message was sent to `peer`.
    pub fn register_message_sent(&mut self, protocol: &T::Protocol, hash: u64, size: usize) {
        let mut entry = self.protocols.entry(protocol.to_owned()).or_default();

        entry.total_bytes_sent += size;
        entry.total_messages_sent += 1;

        if !entry.unique_messages_sent.insert(hash) {
            entry.redundant_bytes_sent += size;
        }
    }
}

/// Heuristic backend.
pub struct HeuristicsBackend<T: NetworkBackend> {
    /// RX channel for receiving events from [`HeuristicsHandle`].
    rx: mpsc::UnboundedReceiver<HeuristicsEvent<T>>,

    /// TX channel for sending heuristics updates to the WebSocket server.
    ws_tx: mpsc::UnboundedSender<String>,

    /// Connected peers.
    peers: HashMap<T::PeerId, PeerHeuristics<T>>,
}

impl<T: NetworkBackend> HeuristicsBackend<T> {
    /// Create new [`HeuristicsBackend`].
    pub fn new(address: Option<SocketAddr>) -> (Self, HeuristicsHandle<T>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (ws_tx, ws_rx) = mpsc::unbounded_channel();

        // start event loop for a WebSocket server if user provided an address for it
        if let Some(address) = address {
            tokio::spawn(heuristics_server(ws_rx, address));
        }

        (
            Self {
                rx,
                ws_tx,
                peers: HashMap::new(),
            },
            HeuristicsHandle { tx },
        )
    }

    fn handle_event(&mut self, event: HeuristicsEvent<T>) {
        match event {
            HeuristicsEvent::_RegisterInterface { interface: _ } => {
                // TODO: implement
            }
            HeuristicsEvent::LinkInterfaces {
                _first: _,
                _second: _,
            } => {
                // TODO: implement
            }
            HeuristicsEvent::_UnlinkInterfaces {
                first: _,
                second: _,
            } => {
                // TODO: implement
            }
            HeuristicsEvent::RegisterPeer { interface, peer } => {
                self.peers
                    .entry(peer)
                    .or_default()
                    .interfaces
                    .insert(interface);
            }
            HeuristicsEvent::UnregisterPeer { interface, peer } => {
                if let Some(info) = self.peers.get_mut(&peer) {
                    info.interfaces.remove(&interface);
                }
            }
            HeuristicsEvent::MessageReceived {
                _interface: _,
                protocol,
                peer,
                hash,
                size,
            } => {
                if let Some(info) = self.peers.get_mut(&peer) {
                    info.register_message_received(&protocol, hash, size)
                }
            }
            HeuristicsEvent::MessageSent {
                _interface: _,
                protocol,
                peers,
                hash,
                size,
            } => {
                for peer in peers {
                    if let Some(info) = self.peers.get_mut(&peer) {
                        info.register_message_sent(&protocol, hash, size)
                    }
                }
            }
        }
    }

    /// Run the event loop of [`HeuristicsBackend`].
    pub async fn run(mut self) {
        let mut now = Instant::now();

        loop {
            tokio::select! {
                event = self.rx.recv() => {
                    self.handle_event(event.expect("channel to stay open"));
                }
                _ = tokio::time::sleep(Duration::from_secs(UPDATE_INTERVAL)) => {}
            }

            if now.elapsed().as_secs() >= UPDATE_INTERVAL {
                let json =
                    serde_json::to_string(&self.peers).expect("`PeerHeuristics` to serialize");
                let _ = self.ws_tx.send(json);
                now = Instant::now();
            }
        }
    }
}

/// WebSocket server for sending heuristics information to the WebSocket client.
async fn heuristics_server(
    mut rx: mpsc::UnboundedReceiver<String>,
    address: SocketAddr,
) -> crate::Result<()> {
    let server = TcpListener::bind(address).await?;
    let (socket, _addr) = server.accept().await.unwrap();

    tracing::debug!(target: LOG_TARGET, "client connected to heuristics backend");

    let mut ws_stream = tokio_tungstenite::accept_async(socket)
        .await
        .map_err(|err| Error::Custom(err.to_string()))?;

    while let Some(json) = rx.recv().await {
        ws_stream.send(Message::Text(json)).await.unwrap();
    }

    Ok(())
}
