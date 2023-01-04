#![allow(unused)]

use crate::types::PeerId;

use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncReadExt,
    net::TcpListener,
    sync::mpsc::{self, Receiver, Sender},
};

use std::{error::Error, pin::Pin, time::Duration};

const NUM_SYBIL: usize = 3usize;
const TCP_START: u16 = 55_555;

mod sybil;
mod types;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (tx, mut rx) = mpsc::channel(32);

    for i in 0..3 {
        let mut sybil = sybil::Interface::new(
            TcpListener::bind(format!("127.0.0.1:{}", TCP_START + i)).await?,
            tx.clone(),
        );

        tokio::spawn(async move { sybil.run().await });
    }

    loop {
        match rx.recv().await.unwrap() {
            sybil::Event::Connected(_peer) => {}
            sybil::Event::Disconnected(_peer) => {
                todo!();
            }
            sybil::Event::Message(_peer, _msg) => {
                todo!();
            }
        }
    }

    Ok(())
}
