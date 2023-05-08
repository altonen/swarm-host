use crate::backend::NetworkBackend;

use tokio::sync::oneshot;

use std::{net::SocketAddr, time::Duration};

/// Default channel size.
pub const DEFAULT_CHANNEL_SIZE: usize = 50_000;

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

        /// Filter code.
        filter: String,

        /// Poll interval for the installed filter.
        poll_interval: Duration,

        /// Optional executor code that is called to initialize interface parameters.
        preinit: Option<String>,

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

    /// Add custom notification filter to [`crate::filter::Filter`].
    InstallNotificationFilter {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Protocol.
        protocol: T::Protocol,

        /// Filter code.
        filter_code: String,

        // User-specified context that is passed on to the filter.
        context: String,

        /// Result of the unlink operation.
        result: oneshot::Sender<crate::Result<()>>,
    },

    /// Add custom request-response filter to [`crate::filter::Filter`].
    InstallRequestResponseFilter {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Protocol.
        protocol: T::Protocol,

        /// Filter code.
        filter_code: String,

        // User-specified context that is passed on to the filter.
        context: String,

        /// Result of the unlink operation.
        result: oneshot::Sender<crate::Result<()>>,
    },
}
