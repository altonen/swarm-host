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
// TODO: convert `sybil.rs` into a directory
// TODO: create sybil/dummy.rs

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // TODO: clean this code
    let (tx, mut rx) = mpsc::channel(32);
    let mut id = 1;
    let mut entries = HashMap::new();

    for i in 0..3 {
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
            sybil::Event::Connected { peer, protocols } => {
                println!("peer {peer:?} connected, supported protocols {protocols:#?}");
            }
            sybil::Event::Disconnected(_peer) => {
                println!("peer disconnected");
            }
            sybil::Event::Message(_peer, _msg) => {
                println!("peer sent a message");
            }
        }
    }

    Ok(())
}
