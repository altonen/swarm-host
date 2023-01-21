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

// TODO: create PoC where comm with between sybil iface and sybil node work
// TODO: start using logging library
// TODO: document code
// TODO: convert `sybil.rs` into a directory
// TODO: create sybil/dummy.rs
// TODO: unitfy naming
// TODO: create vision of the project's future

const NUM_SYBIL: usize = 3usize;
const TCP_START: u16 = 55_555;

mod sybil;
mod types;

// TODO: find a better way to implement this
// TODO: this needs to be generic over event?
struct SybilEntry {
    // TODO: better type
    id: u8,
    tx_iface: Sender<sybil::Event>,
    tx_node: Sender<sybil::Event>,
}

// TODO: documentation
struct NodeEntry {
    id: PeerId,
    protocols: Vec<String>,
    tx: Sender<(String, String)>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // TODO: clean this code
    let (tx, mut rx) = mpsc::channel(32);
    let mut id = 1;
    let mut entries = HashMap::new();
    let mut nodes = HashMap::new();

    for i in 0..1 {
        let (itx, irx) = mpsc::channel(64);
        let (ntx, nrx) = mpsc::channel(64);

        let mut interface = sybil::Interface::new(
            TcpListener::bind(format!("127.0.0.1:{}", TCP_START + i)).await?,
            irx,
            tx.clone(),
        );

        let mut node = sybil::Node::new(nrx, tx.clone(), Duration::from_secs(5));

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
                println!("peer {peer:?} connected, supported protocols {protocols:#?}");

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
                println!("peer disconnected");

                nodes.remove(&peer);
            }
            sybil::Event::Message { .. } => {
                println!("node sent a message");

                // TODO: send message to sybil nodes
            }
            sybil::Event::SybilMessage {
                message,
                protocol,
                peer,
            } => {
                println!(
                    "sybil node {peer:?} sent a message over protocol '{protocol}': {message}"
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
