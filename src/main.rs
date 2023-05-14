use crate::{
    backend::substrate::{SubstrateBackend, SubstrateParameters},
    error::Error,
    executor::pyo3::PyO3Executor,
    overseer::Overseer,
    rpc::run_server,
};

use clap::{Parser, Subcommand};
use hex;

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

    /// Disable heuristics.
    #[clap(long)]
    disable_heuristics: Option<bool>,

    /// Network backend type.
    #[command(subcommand)]
    backend: Backend,
}

#[derive(Clone, Subcommand)]
enum Backend {
    /// Substrate backend.
    Substrate {
        /// Genesis hash.
        #[clap(long)]
        genesis_hash: String,
    },
}

async fn run_substrate_backend(
    rpc_port: u16,
    disable_heuristics: Option<bool>,
    genesis_hash: &String,
) {
    let disable_heuristics = disable_heuristics.unwrap_or(false);

    let parameters = SubstrateParameters {
        genesis_hash: hex::decode(&genesis_hash[2..]).expect("valid genesis hash"),
    };

    let (overseer, tx) = Overseer::<SubstrateBackend, PyO3Executor<SubstrateBackend>>::new(
        disable_heuristics,
        parameters,
    );
    tokio::spawn(async move { overseer.run().await });

    run_server(
        tx,
        format!("127.0.0.1:{}", rpc_port)
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
        Backend::Substrate { ref genesis_hash } => {
            run_substrate_backend(flags.rpc_port, flags.disable_heuristics, &genesis_hash).await
        }
    }
}
