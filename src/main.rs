#![allow(unused)]

use crate::{backend::NetworkBackendType, types::PeerId};

use clap::Parser;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncReadExt,
    net::TcpListener,
    sync::mpsc::{self, Receiver, Sender},
};

use std::{collections::HashMap, error::Error, pin::Pin, time::Duration};

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

const NUM_SYBIL: usize = 3usize;
const TCP_START: u16 = 55_555;
const LOG_TARGET: &'static str = "swarm-host";

mod backend;
mod sybil;
mod types;

#[derive(Parser)]
struct Flags {
    /// RPC port.
    #[clap(long)]
    rpc_port: u16,

    #[clap(long)]
    backend: NetworkBackendType,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let flags = Flags::parse();

    // initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .expect("to succeed");

    // TODO: start rpc server
    // TODO: start correct backend
    // TODO: start correct interfaces

    Ok(())
}
