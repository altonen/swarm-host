#![allow(unused)]
use crate::types::OverseerEvent;

use futures::stream::Stream;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{io::AsyncWrite, sync::mpsc::Sender};

use std::{fmt::Debug, future::Future, hash::Hash, net::SocketAddr, pin::Pin};

pub mod mockchain;

/// Stream which allows reading events from the interface.
pub type InterfaceEventStream<T> = Pin<Box<dyn Stream<Item = InterfaceEvent<T>> + Send>>;

/// List of supported network backends.
#[derive(clap::ValueEnum, Clone)]
pub enum NetworkBackendType {
    Mockchain,
}

// TODO: how to express capabilities in a generic way?
pub enum InterfaceType {
    /// Interface will masquerade a real node.
    Masquerade,

    /// Interface is backed up by a real node.
    NodeBacked,
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

        // TODO: too leaky
        /// Socket which allows sending messages to the peer.
        socket: Box<dyn AsyncWrite + Send>,

        /// Supported protocols.
        protocols: Vec<T::ProtocolId>,
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
    /// Return reference to the interface ID.
    fn id(&self) -> &T::InterfaceId;

    /// Attempt to establish connection with a remote peer.
    fn connect(&mut self, address: SocketAddr) -> crate::Result<()>;

    /// Attempt to disconnect peer from the interface.
    fn disconnect(&mut self, peer: T::PeerId) -> crate::Result<()>;
}

/// Traits which each network backend must implement.
#[async_trait::async_trait]
pub trait NetworkBackend {
    /// Unique ID identifying a peer.
    type PeerId: Debug + Copy + Clone + Eq + Hash + Send + Sync;

    /// Unique ID identifying the interface.
    type InterfaceId: Debug + Copy + Clone + Eq + Hash + Send + Sync;

    /// Unique ID identifying a protocol.
    type ProtocolId: Debug + Copy + Clone + Send + Sync;

    /// Type identifying a message understood by the backend.
    type Message: Serialize + DeserializeOwned + Debug + Clone + Send + Sync;

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
    ) -> crate::Result<(Self::InterfaceHandle, InterfaceEventStream<Self>)>
    where
        Self: Sized;
}
