#![allow(unused)]

use crate::types::PeerId;

use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncReadExt,
    net::TcpListener,
    sync::mpsc::{self, Receiver, Sender},
};

use std::{collections::HashMap, error::Error, pin::Pin, time::Duration};

// TODO: convert `sybil.rs` into a directory
// TODO: create sybil/dummy.rs
// TODO: add prometheus metrics
// TODO: fix warnings
// TODO: run clippy
// TODO: unitfy naming
// TODO: document code
// TODO: create vision of the project's future

const NUM_SYBIL: usize = 3usize;
const TCP_START: u16 = 55_555;
const LOG_TARGET: &'static str = "swarm-host";

mod sybil;
mod types;

// TODO: find a better way to implement this
// TODO: this needs to be generic over event?
struct SybilEntry {
    // TODO: better type
    id: u8,
    tx_iface: Sender<sybil::Event>,
    tx_node: Sender<sybil::Event>,
    tx_msg: Sender<(String, String)>,
}

// TODO: documentation
struct NodeEntry {
    id: PeerId,
    protocols: Vec<String>,
    tx: Sender<(String, String)>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    // TODO: clean this code
    let (tx, mut rx) = mpsc::channel(32);
    let mut id = 1;
    let mut entries = HashMap::new();
    let mut nodes = HashMap::new();

    for i in 0..1 {
        let (itx, irx) = mpsc::channel(64);
        let (ntx, nrx) = mpsc::channel(64);
        let (mtx, mrx) = mpsc::channel(64);

        let mut interface = sybil::Interface::new(
            TcpListener::bind(format!("127.0.0.1:{}", TCP_START + i)).await?,
            irx,
            tx.clone(),
        );

        let mut node = sybil::Node::new(nrx, mrx, tx.clone(), Duration::from_secs(5));

        tokio::spawn(async move { interface.run().await });
        tokio::spawn(async move { node.run().await });

        let node_id = id;
        id += 1;

        entries.insert(
            id,
            SybilEntry {
                id,
                tx_iface: itx,
                tx_node: ntx,
                tx_msg: mtx,
            },
        );
    }

    loop {
        match rx.recv().await.expect("channel to stay open") {
            sybil::Event::Connected {
                peer,
                protocols,
                tx,
            } => {
                tracing::info!(target: LOG_TARGET, id = peer, protocols = ?protocols, "peer connected");

                nodes.insert(
                    peer,
                    NodeEntry {
                        id: peer,
                        protocols,
                        tx,
                    },
                );
            }
            sybil::Event::Disconnected(peer) => {
                tracing::info!(target: LOG_TARGET, peer = peer, "peer disconnected");

                nodes.remove(&peer);
            }
            sybil::Event::Message {
                protocol,
                message,
                peer,
            } => {
                tracing::trace!(
                    target: LOG_TARGET,
                    id = peer,
                    protocol = protocol,
                    "received message from node"
                );

                for (id, node) in &mut entries {
                    node.tx_msg
                        .send((protocol.clone(), message.clone()))
                        .await
                        .unwrap();
                }
            }
            sybil::Event::SybilMessage {
                message,
                protocol,
                peer,
            } => {
                tracing::trace!(
                    target: LOG_TARGET,
                    id = peer,
                    protocol = protocol,
                    "received message from sybil node"
                );

                for (id, node) in &mut nodes {
                    node.tx
                        .send((protocol.clone(), message.clone()))
                        .await
                        .unwrap();
                }
            }
        }
    }

    Ok(())
}
