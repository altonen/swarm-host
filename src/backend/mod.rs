#![allow(unused)]
use crate::{executor::IntoExecutorObject, types::OverseerEvent};

use futures::stream::Stream;
use pyo3::{prelude::*, AsPyPointer, FromPyObject, IntoPy};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{io::AsyncWrite, sync::mpsc::Sender};

use std::{
    collections::HashSet, fmt::Debug, future::Future, hash::Hash, net::SocketAddr, pin::Pin,
};

pub mod mockchain;
pub mod substrate;

// TODO: `pyo3` stuff should absolutely not be here
// TODO: rename `packet/Packet` to `Notification`
// TODO: rename `PeerId` to `Source`
// TODO: define generic `Address` type

/// Stream which allows reading events from the interface.
pub type InterfaceEventStream<T> = Pin<Box<dyn Stream<Item = InterfaceEvent<T>> + Send>>;

/// List of supported network backends.
#[derive(clap::ValueEnum, Clone)]
pub enum NetworkBackendType {
    Mockchain,
    Substrate,
}

// TODO: how to express capabilities in a generic way?
// TODO: specify how node backed interfaces work?
#[derive(Debug)]
pub enum InterfaceType {
    /// Interface will masquerade a real node.
    Masquerade,

    /// Interface is backed up by a real node.
    NodeBacked,
}

/// Abstraction which allows `swarm-host` to send packets to peer.
#[async_trait::async_trait]
pub trait PacketSink<T: NetworkBackend>: Debug {
    /// Send packet to peer over `protocol`.
    async fn send_packet(
        &mut self,
        protocol: Option<T::Protocol>,
        message: &T::Message,
    ) -> crate::Result<()>;

    /// Send request to peer over `protocol`.
    async fn send_request(
        &mut self,
        protocol: T::Protocol,
        response: T::Request,
    ) -> crate::Result<T::RequestId>;

    /// Send response to peer.
    async fn send_response(
        &mut self,
        request_id: T::RequestId,
        response: T::Response,
    ) -> crate::Result<()>;
}

/// Trait allowing to query the request ID from opaque request.
pub trait IdableRequest<T: NetworkBackend> {
    fn id(&self) -> &T::RequestId;
}

/// Connection received an upgrade.
#[derive(Debug)]
pub enum ConnectionUpgrade<T: NetworkBackend> {
    /// Protocol opened.
    ProtocolOpened {
        /// One or more protocols were opened.
        protocols: HashSet<T::Protocol>,
    },

    /// Protocol closed.
    ProtocolClosed {
        /// One or more protocols were closed.
        protocols: HashSet<T::Protocol>,
    },
}

// TODO: how to express capabilities in a generic way?
// TODO: capabilities should come from python and
//       be transported to `NetworkBackend` in a generic way??
#[derive(Debug)]
pub enum InterfaceEvent<T: NetworkBackend> {
    /// Peer connected to the interface.
    PeerConnected {
        /// Associated interface.
        interface: T::InterfaceId,

        /// ID of the peer who connected.
        peer: T::PeerId,

        /// One or more supported protocols.
        protocols: Vec<T::Protocol>,

        /// Socket which allows sending messages to the peer.
        sink: Box<dyn PacketSink<T> + Send>,
    },

    /// Peer disconnected from the interface.
    PeerDisconnected {
        /// Associated interface
        interface: T::InterfaceId,

        /// Peer who disconnected.
        peer: T::PeerId,
    },

    /// Connection received an upgrade
    ConnectionUpgraded {
        /// Associated interface.
        interface: T::InterfaceId,

        /// ID of the peer who connected.
        peer: T::PeerId,

        /// One or more supported protocols.
        upgrade: ConnectionUpgrade<T>,
    },

    /// Message received from one of the peers
    MessageReceived {
        /// Associated interface.
        interface: T::InterfaceId,

        /// Peer who sent the message.
        peer: T::PeerId,

        /// Protocol.
        protocol: T::Protocol,

        /// Received message.
        message: T::Message,
    },

    /// Request received from one of the peers.
    RequestReceived {
        /// Associated interface.
        interface: T::InterfaceId,

        /// Peer who sent the message.
        peer: T::PeerId,

        /// Protocol.
        protocol: T::Protocol,

        /// Received message.
        request: T::Request,
    },

    /// Response received from one of the peers.
    ResponseReceived {
        /// Associated interface.
        interface: T::InterfaceId,

        /// Peer who sent the message.
        peer: T::PeerId,

        /// Protocol.
        protocol: T::Protocol,

        /// Request ID.
        request_id: T::RequestId,

        /// Received message.
        response: T::Response,
    },
}

/// Abstraction which allows `swarm-host` to maintain connections to remote peers.
pub trait Interface<T: NetworkBackend> {
    /// Return reference to the interface ID.
    fn id(&self) -> &T::InterfaceId;

    /// Get handle to installed filter
    fn filter(
        &self,
        filter_name: &String,
    ) -> Option<
        Box<
            dyn Fn(T::InterfaceId, T::PeerId, T::InterfaceId, T::PeerId, &T::Message) -> bool
                + Send,
        >,
    >;

    /// Attempt to establish connection with a remote peer.
    fn connect(&mut self, address: SocketAddr) -> crate::Result<()>;

    /// Attempt to disconnect peer from the interface.
    fn disconnect(&mut self, peer: T::PeerId) -> crate::Result<()>;
}

/// Traits which each network backend must implement.
#[async_trait::async_trait]
pub trait NetworkBackend: Debug + 'static {
    /// Unique ID identifying a peer.
    type PeerId: Debug
        + Copy
        + Clone
        + Eq
        + Hash
        + Send
        + Sync
        // TODO: remove pyo3 traits
        + for<'a> FromPyObject<'a>
        + IntoPy<PyObject>
        + IntoExecutorObject;

    /// Unique ID identifying the interface.
    type InterfaceId: Serialize
        + DeserializeOwned
        + Debug
        + Copy
        + Clone
        + Eq
        + Hash
        + Ord
        + Send
        + Sync
        // TODO: remove these
        + for<'a> FromPyObject<'a>
        + IntoPy<PyObject>;

    /// Unique ID identifying a request.
    type RequestId: Debug + Copy + Clone + PartialEq + Eq + Hash + Send + Sync;

    /// Unique ID identifying a protocol.
    type Protocol: Serialize
        + DeserializeOwned
        + Debug
        + Clone
        + Hash
        + PartialEq
        + Eq
        + Send
        + Sync;

    /// Type identifying a message understood by the backend.
    // TODO: zzz
    type Message: Serialize
        + DeserializeOwned
        + Debug
        + Clone
        + Send
        + Sync
        // TODO: remove pyo3 trait
        + IntoPy<PyObject>
        + IntoExecutorObject;

    /// Type identifying a request understood by the backend.
    type Request: Debug + Send + Sync + IdableRequest<Self>
    where
        Self: Sized;

    /// Type identifying a response understood by the backend.
    type Response: Debug + Clone + Send + Sync;

    /// Handle which allows communication with a spawned interface.
    type InterfaceHandle: Interface<Self>
    where
        Self: Sized;

    /// Create new `NetworkBackend`.
    fn new() -> Self;

    /// Start new interface for accepting incoming connections.
    ///
    /// Return a handle which allows performing actions on the interface
    /// such as publishing messages or managing peer connections and
    /// a stream which allows reading events from interface.
    async fn spawn_interface(
        &mut self,
        address: SocketAddr,
        interface_type: InterfaceType,
    ) -> crate::Result<(Self::InterfaceHandle, InterfaceEventStream<Self>)>
    where
        Self: Sized;
}
