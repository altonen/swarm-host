#![allow(unused)]

use crate::{
    backend::{
        mockchain::types::{InterfaceId, Message, PeerId, ProtocolId},
        Interface, InterfaceEvent, InterfaceEventStream, InterfaceType, NetworkBackend,
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

mod masquerade;
mod node_backed;
pub mod types;

const LOG_TARGET: &'static str = "mockchain";

// TODO: ugly
enum P2pType {
    Masquerade(masquerade::P2p),
    NodeBacked(node_backed::P2p),
}

/// Interface handle.
pub struct MockchainHandle {
    /// Unique ID of the interface.
    id: InterfaceId,

    // TODO: is there need to store p2p here?
    /// Peer-to-peer functionality.
    p2p_type: P2pType,

    /// Connected peers.
    peers: HashMap<PeerId, ()>,
}

impl MockchainHandle {
    /// Create new [`MockchainHandle`].
    pub async fn new(
        id: InterfaceId,
        address: SocketAddr,
        interface_type: InterfaceType,
    ) -> crate::Result<(Self, InterfaceEventStream<MockchainBackend>)> {
        let listener = TcpListener::bind(address).await?;
        let (iface_tx, iface_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        let p2p_type = match interface_type {
            InterfaceType::Masquerade => {
                P2pType::Masquerade(masquerade::P2p::start(iface_tx, listener, id))
            }
            InterfaceType::NodeBacked => {
                P2pType::NodeBacked(node_backed::P2p::start(iface_tx, listener, id))
            }
        };

        Ok((
            Self {
                id,
                p2p_type,
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

    /// Get installed filter, if it exists.
    fn filter(
        &self,
        filter_name: &String,
    ) -> Option<
        Box<
            dyn Fn(
                    <MockchainBackend as NetworkBackend>::InterfaceId,
                    <MockchainBackend as NetworkBackend>::PeerId,
                    <MockchainBackend as NetworkBackend>::InterfaceId,
                    <MockchainBackend as NetworkBackend>::PeerId,
                    &<MockchainBackend as NetworkBackend>::Message,
                ) -> bool
                + Send,
        >,
    > {
        if filter_name == "test_filter" {
            return Some(Box::new(
                |src_iface, src_peer, dst_iface, dst_peer, message| {
                    !matches!(message, Message::Vote(_))
                },
            ));
        }

        None
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
    type Protocol = ProtocolId;
    type Message = Message;
    type Request = Message;
    type Response = Message;
    type InterfaceHandle = MockchainHandle;

    /// Create new [`MockchainBackend`].
    fn new() -> Self {
        MockchainBackend::new()
    }

    /// Spawn new interface for [`MockchainBackend`].
    async fn spawn_interface(
        &mut self,
        address: SocketAddr,
        interface_type: InterfaceType,
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

        match interface_type {
            InterfaceType::Masquerade => {
                let id = self.next_interface_id();

                MockchainHandle::new(id, address, interface_type)
                    .await
                    .map(|(handle, stream)| {
                        self.interfaces.insert(address, id);
                        (handle, stream)
                    })
            }
            InterfaceType::NodeBacked => {
                todo!();
            }
        }
    }
}
