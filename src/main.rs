#![allow(unused)]

use crate::{
    backend::{mockchain::MockchainBackend, substrate::SubstrateBackend, NetworkBackendType},
    error::Error,
    executor::pyo3::PyO3Executor,
    overseer::Overseer,
    rpc::run_server,
};

use clap::Parser;
use tracing_subscriber::fmt::SubscriberBuilder;

use std::net::SocketAddr;

// TODO: implement node-backed interfaces
// TODO: introduce more generic `NetworkBackend::Source` instead of `NetworkBackend::PeerId`
// TODO: `InterfaceId` doesn't need to be part of `NetworkBackend`
// TODO: add prometheus metrics
// TODO: fix warnings
// TODO: run clippy
// TODO: unitfy naming
// TODO: document code
// TODO: get rid of unneeded dependencies

mod backend;
mod error;
mod executor;
mod filter;
mod heuristics;
mod overseer;
mod rpc;
mod types;
mod utils;

/// Global result type.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Parser)]
struct Flags {
    /// RPC port.
    #[clap(long)]
    rpc_port: u16,

    /// WebSocket port.
    #[clap(long)]
    ws_port: Option<u16>,

    /// Network backend type.
    #[clap(long)]
    backend: NetworkBackendType,
}

async fn run_mockchain_backend(flags: Flags) {
    let ws_address = flags.ws_port.map(|port| {
        format!("127.0.0.1:{}", port)
            .parse::<SocketAddr>()
            .expect("valid address")
    });

    let (mut overseer, tx) =
        Overseer::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(ws_address);
    tokio::spawn(async move { overseer.run().await });

    run_server(
        tx,
        format!("127.0.0.1:{}", flags.rpc_port)
            .parse::<SocketAddr>()
            .expect("valid address"),
    )
    .await;
}

async fn run_substrate_backend(flags: Flags) {
    let ws_address = flags.ws_port.map(|port| {
        format!("127.0.0.1:{}", port)
            .parse::<SocketAddr>()
            .expect("valid address")
    });

    let (mut overseer, tx) =
        Overseer::<SubstrateBackend, PyO3Executor<SubstrateBackend>>::new(ws_address);
    tokio::spawn(async move { overseer.run().await });

    run_server(
        tx,
        format!("127.0.0.1:{}", flags.rpc_port)
            .parse::<SocketAddr>()
            .expect("valid address"),
    )
    .await;
}

#[tokio::main]
async fn main() {
    let flags = Flags::parse();

    // initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .expect("to succeed");

    // capture `SIGINT` and exit process
    // see https://github.com/PyO3/pyo3/issues/2576 for more details
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        std::process::exit(0);
    });

    match flags.backend {
        NetworkBackendType::Mockchain => run_mockchain_backend(flags).await,
        NetworkBackendType::Substrate => run_substrate_backend(flags).await,
    }
}
