#![allow(unused)]

use crate::{
    backend::{mockchain::MockchainBackend, NetworkBackendType},
    error::Error,
    overseer::Overseer,
    rpc::run_server,
};

use clap::Parser;

use std::net::SocketAddr;

// TODO: think about architecture for this project:
//  - swarm-host is started
//  - peers connect to it
//  - swarm-host relays traffic between the nodes
//    - full bypass mode
//    - how to install custom filters for traffic?

// TODO: implement compat layer for swarm-host for this blockchain
// TODO: create sybil/dummy.rs
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

#[tokio::main]
async fn main() {
    let flags = Flags::parse();

    // initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .expect("to succeed");

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
