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
}
