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
    Io(io::Error),

    #[error("Serde CBOR error: `{0}`")]
    SerdeCbor(serde_cbor::Error),

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
    Substrate(sc_network_common::error::Error),

    #[error("Custom error: `{0}`")]
    Custom(String),

    #[error("Filter error: `{0}`")]
    FilterError(FilterError),

    #[error("Executor error: `{0}`")]
    ExecutorError(ExecutorError),
}

#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Runtime error: `{0}`")]
    RuntimeError(String),

    #[error("Executor does not exist")]
    ExecutorDoesntExist,

    #[error("Filter does not exist")]
    FilterDoesntExist,
}

#[derive(Error, Debug)]
pub enum FilterError {
    #[error("Invalid filter code: `{0}`")]
    InvalidFilter(String),
}

impl From<pyo3::PyErr> for Error {
    fn from(error: pyo3::PyErr) -> Self {
        Error::ExecutorError(ExecutorError::RuntimeError(error.to_string()))
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<serde_cbor::Error> for Error {
    fn from(error: serde_cbor::Error) -> Self {
        Error::SerdeCbor(error)
    }
}

impl From<sc_network_common::error::Error> for Error {
    fn from(error: sc_network_common::error::Error) -> Self {
        Error::Substrate(error)
    }
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Error::InvalidAddress(address1), Error::InvalidAddress(address2)) => {
                address1 == address2
            }
            (Error::AddressInUse(address1), Error::AddressInUse(address2)) => address1 == address2,
            (Error::Io(error1), Error::Io(error2)) => error1.kind() == error2.kind(),
            (Error::SerdeCbor(error1), Error::SerdeCbor(error2)) => {
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
