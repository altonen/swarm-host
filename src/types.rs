use crate::backend::NetworkBackend;

use thiserror::Error;
use tokio::sync::oneshot;

use std::net::SocketAddr;

/// Default channel size.
pub const DEFAULT_CHANNEL_SIZE: usize = 64;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid address: `{0}`")]
    InvalidAddress(String),

    #[error("Address already in use: `{0}`")]
    AddressInUse(SocketAddr),

    #[error("I/O error: `{0}`")]
    IoError(std::io::Error),

    #[error("Serde CBOR error: `{0}`")]
    SerdeCborError(serde_cbor::Error),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::IoError(error)
    }
}

impl From<serde_cbor::Error> for Error {
    fn from(error: serde_cbor::Error) -> Self {
        Error::SerdeCborError(error)
    }
}

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
