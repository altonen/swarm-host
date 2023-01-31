#![allow(unused)]

use futures::stream::Stream;

use std::{future::Future, net::SocketAddr, pin::Pin};

mod mockchain;

/// List of supported network backends.
#[derive(clap::ValueEnum, Clone)]
pub enum NetworkBackendType {
    Mockchain,
}

// TODO: how to express capabilities in a generic way?
// TODO: capabilities should come from python and
//       be transported to `NetworkBackend` in a generic way??
pub enum InterfaceEvent<T: NetworkBackend> {
    /// Peer connected to the interface.
    PeerConnected {
        /// ID of the peer who connected.
        peer: T::PeerId,

        /// Associated interface.
        interface: T::InterfaceId,
    },

    /// Peer disconnected from the interface.
    PeerDisconnected {
        /// Peer who disconnected.
        peer: T::PeerId,

        /// Associated interface
        interface: T::InterfaceId,
    },

    /// Message received from one of the peers
    MessageReceived {
        /// Peer who sent the message.
        peer: T::PeerId,

        /// Associated interface.
        interface: T::InterfaceId,

        /// Received message.
        message: T::Message,
    },
}

/// Abstraction which allows `swarm-host` to maintain connections to remote peers.
pub trait Interface<T: NetworkBackend> {
    type InterfaceId: Clone;

    /// Unique ID identifying the interface.
    fn id(&self) -> &T::InterfaceId;

    /// Attempt to establish connection with a remote peer.
    fn connect(&mut self, address: SocketAddr) -> crate::Result<()>;

    /// Attempt to disconnect peer from the interface.
    fn disconnect(&mut self, peer: T::PeerId) -> crate::Result<()>;

    /// Get access to the event stream of the interface.
    fn event_stream(&self) -> Pin<Box<dyn Future<Output = InterfaceEvent<T>>>>;
}

/// Traits which each network backend must implement.
pub trait NetworkBackend {
    /// Unique ID identifying a peer.
    type PeerId: Clone;

    /// Unique ID identifying the interface.
    type InterfaceId: Clone;

    /// Type identifying a message understood by the backend.
    // TODO: Serialize + Deserialize?
    type Message: Clone;

    /// Handle which allows communication with a spawned interface.
    type InterfaceHandle: Unpin + Interface<Self> + Stream<Item = InterfaceEvent<Self>>
    where
        Self: Sized;

    /// Start new interface for accepting incoming connections.
    fn spawn_interface(&mut self, address: SocketAddr) -> crate::Result<Self::InterfaceHandle>
    where
        Self: Sized;
}
