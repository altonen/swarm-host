#![allow(unused)]

use crate::{
    backend::{mockchain::types::Message, Interface, InterfaceEvent, NetworkBackend},
    ensure,
    types::{Error, OverseerEvent, DEFAULT_CHANNEL_SIZE},
};

use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::mpsc::{self, Receiver, Sender},
};
use tokio_stream::wrappers::ReceiverStream;

use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::Debug,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

mod p2p;
mod types;

const LOG_TARGET: &'static str = "mockchain";

// TODO: move all type declarations to `type.rs`
// TODO: move code from `p2p` to here
// TODO: redesign event system for the p2p implementation
// TODO: communicate directly with overseer?

/// Unique ID identifying the interface.
type InterfaceId = usize;

/// Unique ID identifying the peer.
type PeerId = u64;

/// Supported protocols.
#[derive(Debug, Serialize, Deserialize)]
enum ProtocolId {
    /// Transaction protocol.
    Transaction,

    /// Block protocol.
    Block,

    /// Peer exchange protocol.
    PeerExchange,
}

#[derive(Debug, Serialize, Deserialize)]
struct Handshake {
    /// Unique ID of the peer.
    peer: PeerId,

    /// Supported protocols of the peer.
    protocols: Vec<ProtocolId>,
}

#[derive(Debug)]
pub enum ConnectionType {
    Inbound,
    Outbound,
}

// TODO: rename to something else?
#[derive(Debug)]
enum PeerEvent {
    PeerConnected {
        peer: PeerId,
        protocols: Vec<ProtocolId>,
        stream: OwnedWriteHalf,
    },
}

enum PeerState {
    /// Handshakes have not been exchanged between the peers.
    Uninitialized {
        stream: TcpStream,
        connection_type: ConnectionType,
    },

    /// Handshakes have been exchanged between the peers.
    Initialized {
        /// Peer ID.
        peer: PeerId,

        /// Reader half of the TCP stream.
        stream: OwnedReadHalf,
    },
}

struct Peer;

impl Peer {
    // TODO: too many params? Refactor
    pub async fn start(
        overseer_tx: Sender<OverseerEvent<MockchainBackend>>,
        iface_tx: Sender<PeerEvent>,
        mut stream: TcpStream,
        connection_type: ConnectionType,
        iface_id: InterfaceId,
    ) -> crate::Result<void::Void> {
        let mut buf = vec![0u8; 8 * 1024];

        if let ConnectionType::Inbound = connection_type {
            let handshake = serde_cbor::to_vec(&Handshake {
                peer: 0u64,
                protocols: vec![
                    ProtocolId::Transaction,
                    ProtocolId::Block,
                    ProtocolId::PeerExchange,
                ],
            })?;

            stream.write(&handshake).await?;
        }

        let nread = stream.read(&mut buf).await?;
        let handshake: Handshake = serde_cbor::from_slice(&buf[..nread])?;

        tracing::debug!(
            target: LOG_TARGET,
            handshake = ?handshake,
            "received handshake from peer"
        );

        // TODO: verify that the peers agree on at least one protocol
        let (mut read, write) = stream.into_split();

        iface_tx
            .send(PeerEvent::PeerConnected {
                peer: handshake.peer,
                protocols: handshake.protocols,
                stream: write,
            })
            .await
            .expect("channel to stay open");

        loop {
            let nread = read.read(&mut buf).await?;
            match serde_cbor::from_slice(&buf[..nread]) {
                Ok(message) => {
                    overseer_tx
                        .send(OverseerEvent::Message {
                            peer: handshake.peer,
                            interface: iface_id,
                            message,
                        })
                        .await
                        .expect("channel to stay open");
                }
                Err(err) => tracing::warn!(
                    target: LOG_TARGET,
                    peer = handshake.peer,
                    interface = ?iface_id,
                    err = ?err,
                    "peer send an invalid message",
                ),
            }
        }
    }
}

// TODO: move this code to `MockchainHandle`
struct P2p;

impl P2p {
    pub fn start(
        overseer_tx: Sender<OverseerEvent<MockchainBackend>>,
        iface_tx: Sender<PeerEvent>,
        listener: TcpListener,
        iface_id: InterfaceId,
    ) -> Self {
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Err(err) => tracing::error!(
                        target: LOG_TARGET,
                        err = ?err,
                        "failed to accept connection"
                    ),
                    Ok((stream, address)) => {
                        tracing::debug!(target: LOG_TARGET, address = ?address, "peer connected");

                        let iface_tx_copy = iface_tx.clone();
                        let overseer_tx_copy = overseer_tx.clone();

                        tokio::spawn(async move {
                            if let Err(err) = Peer::start(
                                overseer_tx_copy,
                                iface_tx_copy,
                                stream,
                                ConnectionType::Inbound,
                                iface_id,
                            )
                            .await
                            {
                                tracing::error!(
                                    target: LOG_TARGET,
                                    err = ?err,
                                    "failed to handle peer connection",
                                );
                            }
                        });
                    }
                }
            }
        });

        Self {}
    }
}

pub struct MockchainHandle {
    /// Unique ID of the interface.
    id: InterfaceId,

    /// Peer-to-peer functionality.
    p2p: P2p,

    /// Event streams.
    event_streams: Vec<Sender<InterfaceEvent<MockchainBackend>>>,
}

impl MockchainHandle {
    pub async fn new(
        id: InterfaceId,
        address: SocketAddr,
        overseer_tx: Sender<OverseerEvent<MockchainBackend>>,
    ) -> crate::Result<Self> {
        let listener = TcpListener::bind(address).await?;
        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let p2p = P2p::start(overseer_tx, tx, listener, id);

        Ok(Self {
            id,
            p2p,
            event_streams: Vec::new(),
        })
    }
}

impl Interface<MockchainBackend> for MockchainHandle {
    fn id(&self) -> &<MockchainBackend as NetworkBackend>::InterfaceId {
        &self.id
    }

    fn connect(&mut self, address: SocketAddr) -> crate::Result<()> {
        todo!();
    }

    fn disconnect(
        &mut self,
        peer: <MockchainBackend as NetworkBackend>::PeerId,
    ) -> crate::Result<()> {
        todo!();
    }

    fn event_stream(
        &mut self,
    ) -> Pin<Box<dyn Stream<Item = InterfaceEvent<MockchainBackend>> + Send>> {
        tracing::trace!(target: LOG_TARGET, interface = self.id, "get event stream");

        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        self.event_streams.push(tx);

        Box::pin(ReceiverStream::new(rx))
    }
}

#[derive(Debug)]
pub struct MockchainBackend {
    next_iface_id: usize,
    interfaces: HashMap<SocketAddr, InterfaceId>,
}

impl MockchainBackend {
    /// Create new [`MockchainBackend`].
    pub fn new() -> Self {
        Self {
            next_iface_id: 0usize,
            interfaces: HashMap::new(),
        }
    }

    /// Allocate ID for new interface.
    pub fn next_interface_id(&mut self) -> usize {
        let iface_id = self.next_iface_id;
        self.next_iface_id += 1;
        iface_id
    }
}

#[async_trait::async_trait]
impl NetworkBackend for MockchainBackend {
    type PeerId = PeerId;
    type InterfaceId = InterfaceId;
    type Message = Message;
    type InterfaceHandle = MockchainHandle;

    /// Create new [`MockchainBackend`].
    fn new() -> Self {
        MockchainBackend::new()
    }

    /// Spawn new interface for [`MockchainBackend`].
    async fn spawn_interface(
        &mut self,
        address: SocketAddr,
        overseer_tx: Sender<OverseerEvent<Self>>,
    ) -> crate::Result<Self::InterfaceHandle> {
        ensure!(
            !self.interfaces.contains_key(&address),
            Error::AddressInUse(address),
        );

        tracing::debug!(
            target: LOG_TARGET,
            address = ?address,
            "create interface"
        );

        let id = self.next_interface_id();
        MockchainHandle::new(id, address, overseer_tx)
            .await
            .map(|handle| {
                self.interfaces.insert(address, id);
                handle
            })
    }
}
