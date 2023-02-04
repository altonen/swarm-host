#![allow(unused)]

use crate::{
    backend::{
        mockchain::types::Message, Interface, InterfaceEvent, InterfaceEventStream, NetworkBackend,
    },
    ensure,
    error::Error,
    types::{OverseerEvent, DEFAULT_CHANNEL_SIZE},
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

mod types;

const LOG_TARGET: &'static str = "mockchain";

// TODO: move all type declarations to `type.rs`

/// Unique ID identifying the interface.
type InterfaceId = usize;

/// Unique ID identifying the peer.
type PeerId = u64;

/// Supported protocols.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum ProtocolId {
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
    /// Local node received a connection and is expecting to
    /// read a handshake from the socket as first message.
    Inbound,

    /// Local node initiated the connection and must send a handshake
    /// message to remote node before doing anything else.
    Outbound,
}

struct Peer;

impl Peer {
    /// Start task for a remote peer.
    // TODO: too many params? Refactor
    pub async fn start(
        iface_tx: Sender<InterfaceEvent<MockchainBackend>>,
        mut stream: TcpStream,
        connection_type: ConnectionType,
        iface_id: InterfaceId,
    ) -> crate::Result<()> {
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

        // TODO: use `expect()` when the leaky abstraction of `socket` is fixed
        if iface_tx
            .send(InterfaceEvent::PeerConnected {
                peer: handshake.peer,
                interface: iface_id,
                protocols: handshake.protocols,
                socket: Box::new(write),
            })
            .await
            .is_err()
        {
            panic!("essential task shut down");
        }

        loop {
            let nread = read.read(&mut buf).await?;

            if nread == 0 {
                tracing::debug!(
                    target: LOG_TARGET,
                    peer = handshake.peer,
                    interface = ?iface_id,
                    "connection closed to peer",
                );

                // TODO: use `expect()` when the leaky abstraction of `socket` is fixed
                if iface_tx
                    .send(InterfaceEvent::PeerDisconnected {
                        peer: handshake.peer,
                        interface: iface_id,
                    })
                    .await
                    .is_err()
                {
                    panic!("essential task shut down");
                }

                return Ok(());
            }

            match serde_cbor::from_slice::<Message>(&buf[..nread]) {
                Ok(message) => {
                    // TODO: use `expect()` when the leaky abstraction of `socket` is fixed
                    if iface_tx
                        .send(InterfaceEvent::MessageReceived {
                            peer: handshake.peer,
                            interface: iface_id,
                            message,
                        })
                        .await
                        .is_err()
                    {
                        panic!("essential task shut down");
                    }
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
    /// Start the P2P functionality.
    pub fn start(
        iface_tx: Sender<InterfaceEvent<MockchainBackend>>,
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
                        tokio::spawn(async move {
                            if let Err(err) = Peer::start(
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

/// Interface handle.
pub struct MockchainHandle {
    /// Unique ID of the interface.
    id: InterfaceId,

    /// Peer-to-peer functionality.
    p2p: P2p,

    /// Connected peers.
    peers: HashMap<PeerId, ()>,
}

impl MockchainHandle {
    /// Create new [`MockchainHandle`].
    pub async fn new(
        id: InterfaceId,
        address: SocketAddr,
    ) -> crate::Result<(Self, InterfaceEventStream<MockchainBackend>)> {
        let listener = TcpListener::bind(address).await?;
        let (iface_tx, iface_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let p2p = P2p::start(iface_tx, listener, id);

        Ok((
            Self {
                id,
                p2p,
                peers: HashMap::new(),
            },
            Box::pin(ReceiverStream::new(iface_rx)),
        ))
    }
}

impl Interface<MockchainBackend> for MockchainHandle {
    /// Get ID of the interface.
    fn id(&self) -> &<MockchainBackend as NetworkBackend>::InterfaceId {
        &self.id
    }

    /// Connect to peer.
    fn connect(&mut self, address: SocketAddr) -> crate::Result<()> {
        todo!();
    }

    /// Disconnect peer.
    fn disconnect(
        &mut self,
        peer: <MockchainBackend as NetworkBackend>::PeerId,
    ) -> crate::Result<()> {
        todo!();
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
    type ProtocolId = ProtocolId;
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
    ) -> crate::Result<(Self::InterfaceHandle, InterfaceEventStream<Self>)> {
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
        MockchainHandle::new(id, address)
            .await
            .map(|(handle, stream)| {
                self.interfaces.insert(address, id);
                (handle, stream)
            })
    }
}
