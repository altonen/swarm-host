#![allow(unused)]

use tokio::{
    io::AsyncReadExt,
    net::TcpListener,
    sync::mpsc::{self, Receiver, Sender},
};

use std::{error::Error, pin::Pin};

// TODO: inform `swarm-host` when node connects to one of the Sybil interfaces
// TODO: start listening to data from this interface

const NUM_SYBIL: usize = 3usize;
const TCP_START: u16 = 55_555;

/// Peer ID.
type PeerId = u64;

enum Event {
    /// Message received from peer.
    Message(Vec<u8>),
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
            Event::Message(msg) => {
                todo!();
            }
        }
    }

    Ok(())
}
