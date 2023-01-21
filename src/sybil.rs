use crate::types::PeerId;

use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
};

use std::{error::Error, pin::Pin, time::Duration};

// TODO: does peer really need a task?

/// TODO: documentation
#[derive(Debug)]
pub enum Event {
    /// Peer connected.
    Connected {
        /// Peer ID.
        peer: PeerId,

        /// Supported protocols.
        protocols: Vec<String>,

        /// TX channel for communication with the peer.
        tx: Sender<(String, String)>,
    },

    /// Peer disconnected.
    Disconnected(PeerId),

    /// Message received from peer to a Sybil interface.
    Message {
        /// Peer ID.
        peer: PeerId,

        /// Protocol of the message.
        protocol: String,

        /// Received message.
        message: String,
    },

    /// Message received from one of the Sybil nodes.
    SybilMessage {
        /// Peer ID.
        peer: PeerId,

        /// Protocol of the message.
        protocol: String,

        /// Received message.
        message: String,
    },
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

    /// TX channel for sender messages from `swarm-host`.
    tx_msg: Sender<(String, String)>,

    /// RX channel for receiving messages from `swarm-host`.
    rx_msg: Receiver<(String, String)>,

    /// Peer state.
    state: PeerState,
}

impl Peer {
    pub fn new(socket: TcpStream, tx: Sender<Event>) -> Self {
        let (tx_msg, rx_msg) = mpsc::channel(64);

        Self {
            socket,
            tx,
            tx_msg,
            rx_msg,
            state: PeerState::Uninitialized,
        }
    }

    pub async fn run(mut self) {
        let mut buf = vec![0u8; 1024];

        loop {
            tokio::select! {
                result = self.socket.read(&mut buf) => match result {
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
                                // TODO: remove unwraps
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
                                    .send(Event::Connected {
                                        peer,
                                        tx: self.tx_msg.clone(),
                                        protocols: protocols.clone(),
                                    })
                                    .await
                                    .expect("channel to stay open");
                                self.state = PeerState::Initialized { peer, protocols };
                            }
                            PeerState::Initialized {
                                peer,
                                ref protocols,
                            } => {
                                let payload =
                                    std::str::from_utf8(&buf[..nread - 1]).expect("valid utf-8 string");
                                let message = payload.split(",").collect::<Vec<&str>>();
                                let protocol = message.get(0).expect("protocol to exist").to_string();
                                let message = message.get(1).expect("message to exist");

                                if protocols.iter().any(|proto| proto == &protocol) {
                                    self.tx
                                        .send(Event::Message {
                                            peer,
                                            protocol,
                                            message: message.to_string(),
                                        })
                                        .await
                                        .expect("channel to stay open");
                                } else {
                                    println!("received message from unknown protocol. peer {peer:?}, protocol {protocol:?}");
                                }
                            }
                        }
                    }
                    Err(err) => {
                        println!("failed to read from opened stream: {err:?}");
                    }
                },
                result = self.rx_msg.recv() => match result {
                    Some((protocol, message)) => {
                        println!("send message to peer");
                        self.socket.write(message.as_bytes()).await.unwrap();
                    }
                    None => panic!("should not fail"),
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
    rx_msg: Receiver<(String, String)>,
    tx: Sender<Event>,
    id: PeerId,
    timeout: Duration,
}

impl Node {
    /// Create a new Sybil node.
    pub fn new(
        rx: Receiver<Event>,
        rx_msg: Receiver<(String, String)>,
        tx: Sender<Event>,
        timeout: Duration,
    ) -> Self {
        let id = rand::thread_rng().gen::<PeerId>();

        println!("starting sybil node {id}");

        Self {
            rx,
            rx_msg,
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
                result = self.rx_msg.recv() => match result {
                    Some((protocol, message)) => {
                        println!("sybil node received message from protocol {protocol}: {message}");
                    }
                    None => panic!("expect channel to stay open"),
                },
                _ = tokio::time::sleep(self.timeout) => {
                    self.tx.send(Event::SybilMessage {
                        peer: self.id,
                        protocol: String::from("/proto/1.0.1"),
                        message: format!("hello from node {}", self.id),
                    })
                    .await
                    .expect("channel to stay to open");
                }
            }
        }
    }
}
