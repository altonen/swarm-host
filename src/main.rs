#![allow(unused)]

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

// TODO: create dummy sybil node which just receives data and generates random data back

/// Peer ID.
type PeerId = u64;

enum Event {
    /// Peer connected.
    Connected(PeerId),

    /// Peer disconnected.
    Disconnected(PeerId),

    /// Message received from peer.
    Message(PeerId, Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
enum Message {
    /// Handshake message containing peer information.
    Handshake {
        /// Peer ID.
        peer: PeerId,

        /// Supported protocols
        protocols: Vec<String>,
    },
}

/// Sybil interface.
///
/// Provides an interface for honest nodes to connect to. Relays
/// traffic betwee
struct Sybil {
    listener: TcpListener,
    tx: Sender<Event>,
}

impl Sybil {
    /// Create new [`Sybil`] interface.
    pub fn new(listener: TcpListener, tx: Sender<Event>) -> Self {
        Self { listener, tx }
    }

    /// Start running the [`Sybil`] interface event loop.
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                result = self.listener.accept() => match result {
                    Err(err) => println!("failed to accept connection: {err:?}"),
                    Ok((mut stream, address)) => {
                        println!("accepted connection from remote peer, address {address:?}");

                        tokio::spawn(async move {
                            let mut buf = vec![0u8; 1024];

                            loop {
                                match stream.read(&mut buf).await {
                                    Ok(nread) if nread == 0 => {
                                        println!("close socket");
                                        break
                                    }
                                    Ok(_nread) => {
                                        println!("read something: {}", std::str::from_utf8(&buf).unwrap());
                                    }
                                    Err(err) => {
                                        println!("failed to read from opened stream: {err:?}");
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }
    }
}

struct SybilNode {
    rx: Receiver<Event>,
    id: u8,
    timeout: Duration,
}

impl SybilNode {
    pub fn new(rx: Receiver<Event>, timeout: Duration) -> Self {
        let id = rand::thread_rng().gen::<u8>();

        println!("starting sybil node {id}");

        Self { rx, timeout, id }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                event = self.rx.recv() => match event {
                    Some(event) => match event {
                        _ => todo!(),
                    }
                    None => break,
                },
                _ = tokio::time::sleep(self.timeout) => {
                    println!("sybil node {} generates random message", self.id);
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (tx, mut rx) = mpsc::channel(32);

    for i in 0..3 {
        let mut sybil = Sybil::new(
            TcpListener::bind(format!("127.0.0.1:{}", TCP_START + i)).await?,
            tx.clone(),
        );

        tokio::spawn(async move { sybil.run().await });
    }

    loop {
        match rx.recv().await.unwrap() {
            Event::Connected(_peer) => {}
            Event::Disconnected(_peer) => {
                todo!();
            }
            Event::Message(_peer, _msg) => {
                todo!();
            }
        }
    }

    Ok(())
}
