use thiserror::Error;
use tokio::sync::oneshot;

use std::net::SocketAddr;

/// Default channel size.
pub const DEFAULT_CHANNEL_SIZE: usize = 64;

/// Interface ID.
///
/// Used to distinguish between different interfaces `swarm-host` provides.
pub type InterfaceId = u8;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid address: `{0}`")]
    InvalidAddress(String),

    #[error("Address already in use: `{0}`")]
    AddressInUse(SocketAddr),

    #[error("I/O error: `{0}`")]
    IoError(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::IoError(error)
    }
}

/// Events that sent from `RPC` to `Overseer`.
pub enum OverseerEvent {
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
        result: oneshot::Sender<crate::Result<InterfaceId>>,
    },
}
