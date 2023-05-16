use crate::{
    backend::{
        mockchain::types::{
            InterfaceId, Message, PeerId, ProtocolId, Request, RequestId, Response,
        },
        Interface, InterfaceEventStream, InterfaceType, NetworkBackend,
    },
    ensure,
    error::Error,
    types::DEFAULT_CHANNEL_SIZE,
};

use serde::Serialize;
use tokio::{net::TcpListener, sync::mpsc};
use tokio_stream::wrappers::ReceiverStream;

use std::{collections::HashMap, fmt::Debug, net::SocketAddr};

mod masquerade;
pub mod types;

const LOG_TARGET: &str = "mockchain";

/// Interface handle.
pub struct MockchainHandle {
    /// Unique ID of the interface.
    id: InterfaceId,

    /// Unique peer ID of the interface.
    peer_id: PeerId,

    /// Connected peers.
    _peers: HashMap<PeerId, ()>,
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

        match interface_type {
            InterfaceType::Masquerade => {
                let _ = masquerade::P2p::start(iface_tx, listener, id);
            }
        }

        Ok((
            Self {
                id,
                peer_id: id as PeerId,
                _peers: HashMap::new(),
            },
            Box::pin(ReceiverStream::new(iface_rx)),
        ))
    }
}

#[async_trait::async_trait]
impl Interface<MockchainBackend> for MockchainHandle {
    /// Get ID of the interface.
    fn interface_id(&self) -> &<MockchainBackend as NetworkBackend>::InterfaceId {
        &self.id
    }

    /// Get the peer ID of the interface.
    fn peer_id(&self) -> &<MockchainBackend as NetworkBackend>::PeerId {
        &self.peer_id
    }

    /// Connect to peer.
    async fn connect(
        &mut self,
        _peer: <MockchainBackend as NetworkBackend>::PeerId,
    ) -> crate::Result<()> {
        todo!();
    }

    /// Disconnect peer.
    async fn disconnect(
        &mut self,
        _peer: <MockchainBackend as NetworkBackend>::PeerId,
    ) -> crate::Result<()> {
        todo!();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    type RequestId = RequestId;
    type Protocol = ProtocolId;
    type Message = Message;
    type Request = Request;
    type Response = Response;
    type InterfaceHandle = MockchainHandle;
    type NetworkParameters = ();
    type InterfaceParameters = usize;

    /// Create new [`MockchainBackend`].
    fn new(_parameters: Self::NetworkParameters) -> Self {
        MockchainBackend::new()
    }

    /// Spawn new interface for [`MockchainBackend`].
    async fn spawn_interface(
        &mut self,
        address: SocketAddr,
        interface_type: InterfaceType,
        parameters: Option<Self::InterfaceParameters>,
    ) -> crate::Result<(Self::InterfaceHandle, InterfaceEventStream<Self>)> {
        ensure!(
            !self.interfaces.contains_key(&address),
            Error::AddressInUse(address),
        );

        tracing::debug!(
            target: LOG_TARGET,
            ?address,
            ?parameters,
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
        }
    }
}
