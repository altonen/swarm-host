use crate::backend::NetworkBackend;

use tokio::sync::oneshot;

use std::net::SocketAddr;

/// Default channel size.
pub const DEFAULT_CHANNEL_SIZE: usize = 64;

/// Events that sent from `RPC` to `Overseer`.
#[derive(Debug)]
pub enum OverseerEvent<T: NetworkBackend> {
    /// Create new interface.
    ///
    /// Create a new interface which other nodes can then connect to
    /// and which allows `Overseer` to modify traffic flow of the network.
    ///
    /// Returns a unique `InterfaceId` which can be used to interact with
    /// the interface via RPC.
    CreateInterface {
        /// Address where to bind the interface.
        address: SocketAddr,

        /// Unique interface ID.
        result: oneshot::Sender<crate::Result<T::InterfaceId>>,
    },

    /// Link interfaces together
    ///
    /// This allows passing messages from one interface to another implicitly
    /// when a message has been received to one of the linked interfaces.
    LinkInterface {
        /// First interface.
        first: T::InterfaceId,

        /// Second interface.
        second: T::InterfaceId,

        /// Result.
        result: oneshot::Sender<crate::Result<()>>,
    },

    /// Unlink interfaces.
    ///
    /// If the interfaces were linked together, remove that link and prevent
    /// any packet flow between the interfaces.
    UnlinkInterface {
        /// First interface.
        first: T::InterfaceId,

        /// Second interface.
        second: T::InterfaceId,

        /// Result of the unlink operation.
        result: oneshot::Sender<crate::Result<()>>,
    },

    /// Add custom filter to [`crate::filter::MessageFilter`].
    ///
    /// Filter is identified by their and the actual function is queried
    /// from the intalled backend. This means that the filter must be have
    /// been compiled as part of the network backend when `swarm-host` was built.
    InstallFilter {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Filter name.
        filter_name: String,

        /// Result of the unlink operation.
        result: oneshot::Sender<crate::Result<()>>,
    },
}
