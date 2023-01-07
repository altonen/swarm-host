use crate::types::PeerId;

use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
};

use std::{error::Error, pin::Pin, time::Duration};

// TODO: implement handshake as a state machine
// TODO: does peer really need a task?

/// TODO: documentation
#[derive(Debug)]
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

// TODO: document
#[derive(Debug)]
enum PeerState {
    // TODO: document
    Uninitialized,

    // TODO: document
    Initialized {
        /// Peer ID.
        peer: PeerId,

        /// Supported protocols.
        protocols: Vec<String>,
    },
}

#[derive(Debug)]
struct Peer {
    /// Socket of the peer.
    socket: TcpStream,

    /// TX channel for communication with `swarm-host`.
    tx: Sender<Event>,

    /// Peer state.
    state: PeerState,
}

impl Peer {
    pub fn new(socket: TcpStream, tx: Sender<Event>) -> Self {
        Self {
            socket,
            tx,
            state: PeerState::Uninitialized,
        }
    }

    pub async fn run(mut self) {
        let mut buf = vec![0u8; 1024];

        loop {
            match self.socket.read(&mut buf).await {
                Ok(nread) if nread == 0 => {
                    println!("close socket");

                    if let PeerState::Initialized { peer, .. } = self.state {
                        println!("peer {peer:?} disconnected, inform `swarm-host`");
                        self.tx
                            .send(Event::Disconnected(peer))
                            .await
                            .expect("channel to stay open");
                    }

                    break;
                }
                Ok(nread) => {
                    match self.state {
                        PeerState::Uninitialized => {
                            // the first message read is the handshake which contains peer information
                            // such as peer ID and supported protocols.
                            //
                            // for this implementation, it's a comma-separated list of values:
                            //   <peer id>,<protocol 1>,<protocol 2>,<protocol 3>
                            let payload =
                                std::str::from_utf8(&buf[..nread - 1]).expect("valid utf-8 string");
                            let handshake = payload.split(",").collect::<Vec<&str>>();
                            let peer = handshake
                                .get(0)
                                .expect("peer id to exist")
                                .parse::<PeerId>()
                                .expect("valid peer id");
                            let protocols: Vec<String> =
                                handshake[1..].into_iter().map(|&x| x.into()).collect();

                            println!("peer with id {peer:?} connected, supported protocols: {protocols:#?}");

                            self.tx
                                .send(Event::Connected(peer))
                                .await
                                .expect("channel to stay open");
                            self.state = PeerState::Initialized { peer, protocols };
                        }
                        PeerState::Initialized { .. } => {
                            // TODO: relay message to swarm host?
                        }
                    }
                }
                Err(err) => {
                    println!("failed to read from opened stream: {err:?}");
                }
            }
        }
    }
}

/// Sybil interface.
///
/// Provides an interface for honest nodes to connect to. Relays
/// traffic betwee TODO: finish
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
                    Ok((stream, address)) => {
                        println!("accepted connection from remote peer, address {address:?}");

                        let tx = self.tx.clone();
                        tokio::spawn(async move { Peer::new(stream, tx).run().await });
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
                    // println!("sybil node {} generates random message", self.id);
                }
            }
        }
    }
}
