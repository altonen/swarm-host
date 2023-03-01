//! `swarm-host` error types

use thiserror::Error;

use std::{io, net::SocketAddr};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid address: `{0}`")]
    InvalidAddress(String),

    #[error("Address already in use: `{0}`")]
    AddressInUse(SocketAddr),

    #[error("I/O error: `{0}`")]
    IoError(io::Error),

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

    #[error("Link does not exist")]
    LinkDoesntExist,

    #[error("Filter already exists")]
    FilterAlreadyExists,

    #[error("Filter does not exit")]
    FilterDoesntExist,

    #[error("Request does not exist")]
    RequestDoesntExist,

    #[error("Substrate error: `{0}`")]
    SubstrateError(sc_network_common::error::Error),

    #[error("Custom error: `{0}`")]
    Custom(String),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IoError(error)
    }
}

impl From<serde_cbor::Error> for Error {
    fn from(error: serde_cbor::Error) -> Self {
        Error::SerdeCborError(error)
    }
}

impl From<sc_network_common::error::Error> for Error {
    fn from(error: sc_network_common::error::Error) -> Self {
        Error::SubstrateError(error)
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
            (Error::LinkDoesntExist, Error::LinkDoesntExist) => true,
            (Error::FilterAlreadyExists, Error::FilterAlreadyExists) => true,
            (Error::FilterDoesntExist, Error::FilterDoesntExist) => true,
            (Error::RequestDoesntExist, Error::RequestDoesntExist) => true,
            _ => false,
        }
    }
}
