use crate::types::PeerId;

use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncReadExt,
    net::TcpListener,
    sync::mpsc::{self, Receiver, Sender},
};

use std::{error::Error, pin::Pin, time::Duration};

/// TODO: documentation
pub enum Event {
    /// Peer connected.
    Connected(PeerId),

    /// Peer disconnected.
    Disconnected(PeerId),

    /// Message received from peer.
    Message(PeerId, Vec<u8>),
}

/// TODO: documentation
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
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
pub struct Interface {
    listener: TcpListener,
    tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl Interface {
    /// Create new [`Sybil`] interface.
    pub fn new(listener: TcpListener, rx: Receiver<Event>, tx: Sender<Event>) -> Self {
        Self { listener, rx, tx }
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

/// TODO: documentation
pub struct Node {
    rx: Receiver<Event>,
    tx: Sender<Event>,
    id: u8,
    timeout: Duration,
}

impl Node {
    /// Create a new Sybil node.
    pub fn new(rx: Receiver<Event>, tx: Sender<Event>, timeout: Duration) -> Self {
        let id = rand::thread_rng().gen::<u8>();

        println!("starting sybil node {id}");

        Self {
            rx,
            tx,
            timeout,
            id,
        }
    }

    /// Start event loop for the sybil node.
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
