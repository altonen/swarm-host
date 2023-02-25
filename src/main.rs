#![allow(unused)]

use crate::{
    backend::{mockchain::MockchainBackend, substrate::SubstrateBackend, NetworkBackendType},
    error::Error,
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
mod filter;
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

    /// Network backend type.
    #[clap(long)]
    backend: NetworkBackendType,
}

async fn run_mockchain_backend(flags: Flags) {
    let (mut overseer, tx) = Overseer::<MockchainBackend>::new();
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
    let (mut overseer, tx) = Overseer::<SubstrateBackend>::new();
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

    match flags.backend {
        NetworkBackendType::Mockchain => run_mockchain_backend(flags).await,
        NetworkBackendType::Substrate => run_substrate_backend(flags).await,
    }
}
