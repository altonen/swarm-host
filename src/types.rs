use crate::backend::NetworkBackend;

use thiserror::Error;
use tokio::sync::oneshot;

use std::net::SocketAddr;

/// Default channel size.
pub const DEFAULT_CHANNEL_SIZE: usize = 64;

// TODO: move to `error.rs`
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

    #[error("Interface already exists")]
    InterfaceAlreadyExists,

    #[error("Peer already exists")]
    PeerAlreadyExists,

    #[error("Interface does not exist")]
    InterfaceDoesntExist,

    #[error("Peer does not exist")]
    PeerDoesntExist,
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

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Error::InvalidAddress(address1), Error::InvalidAddress(address2)) => {
                address1 == address2
            }
            (Error::AddressInUse(address1), Error::AddressInUse(address2)) => address1 == address2,
            (Error::IoError(error1), Error::IoError(error2)) => error1.kind() == error2.kind(),
            (Error::SerdeCborError(error1), Error::SerdeCborError(error2)) => {
                // TODO: verify
                error1.classify() == error2.classify()
            }
            (Error::InterfaceAlreadyExists, Error::InterfaceAlreadyExists) => true,
            (Error::PeerAlreadyExists, Error::PeerAlreadyExists) => true,
            (Error::InterfaceDoesntExist, Error::InterfaceDoesntExist) => true,
            (Error::PeerDoesntExist, Error::PeerDoesntExist) => true,
            _ => false,
        }
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
