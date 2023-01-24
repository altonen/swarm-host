use crate::types::{Block, Command, PeerId, Transaction};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
};

use std::{collections::HashMap, net::SocketAddr};

const LOG_TARGET: &'static str = "p2p";

// TODO: spawn task for each peer
//        - listen for incoming blocks and transations
//        - publish blocks and transations
// TODO: create handshake message instead of sending a comma-separated string

#[derive(Debug, Deserialize, Serialize)]
enum Message {
    Transaction(Transaction),
    Block(Block),
}

/// Commands sent by [`P2p`] to [`Peer`].
#[derive(Debug)]
enum PeerCommand {
    /// Disconnect peer.
    Disconnect,

    /// Publish transaction on the network.
    PublishTransaction(Transaction),

    /// Publish block on the network.
    PublishBlock(Block),
}

/// Events sent by the [`Peer`] to [`P2p`].
#[derive(Debug)]
enum PeerEvent {
    /// Peer disconnected.
    Disconnected {
        peer: PeerId,
    },

    /// Peer connected.
    Connected {
        key: usize,
        peer: PeerId,
        protocols: Vec<String>,
    },

    Message {
        peer: PeerId,
        message: Message,
    },
}

// TODO: document
#[derive(Debug)]
enum PeerState {
    // TODO: document
    Uninitialized {
        key: usize,
    },

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

    /// TX channel for communication.
    tx: Sender<PeerEvent>,

    /// RX channel for receiving commands.
    cmd_rx: Receiver<PeerCommand>,

    /// Peer state.
    state: PeerState,
}

impl Peer {
    pub fn new(
        key: usize,
        socket: TcpStream,
        tx: Sender<PeerEvent>,
        cmd_rx: Receiver<PeerCommand>,
    ) -> Self {
        Self {
            socket,
            tx,
            cmd_rx,
            state: PeerState::Uninitialized { key },
        }
    }

    pub async fn run(mut self) {
        let mut buf = vec![0u8; 1024];

        loop {
            tokio::select! {
                result = self.socket.read(&mut buf) => match result {
                    Ok(nread) if nread == 0 => {
                        tracing::trace!(target: LOG_TARGET, "node closed socket");

                        if let PeerState::Initialized { peer, .. } = self.state {
                            tracing::debug!(target: LOG_TARGET, id = peer, "node disconnected, inform `swarm-host`");

                            self.tx
                                .send(PeerEvent::Disconnected { peer })
                                .await
                                .expect("channel to stay open");
                        }

                        // TODO: notify P2P anyway?
                        break;
                    }
                    Ok(nread) => {
                        match self.state {
                            PeerState::Uninitialized { key } => {
                                // TODO: remove unwraps
                                // the first message read is the handshake which contains peer information
                                // such as peer ID and supported protocols.
                                //
                                // for this implementation, it's a comma-separated list of values:
                                //   <peer id>,<protocol 1>,<protocol 2>,<protocol 3>
                                let payload =
                                    std::str::from_utf8(&buf[..nread]).expect("valid utf-8 string");
                                let handshake = payload.split(",").collect::<Vec<&str>>();
                                let peer = handshake
                                    .get(0)
                                    .expect("peer id to exist")
                                    .parse::<PeerId>()
                                    .expect("valid peer id");
                                let protocols: Vec<String> =
                                    handshake[1..].into_iter().map(|&x| x.into()).collect();

                                tracing::debug!(target: LOG_TARGET, id = peer, protocols = ?protocols, "node connected");

                                self.tx
                                    .send(PeerEvent::Connected {
                                        key,
                                        peer,
                                        protocols: protocols.clone(),
                                    })
                                    .await
                                    .expect("channel to stay open");
                                self.state = PeerState::Initialized { peer, protocols };
                            }
                            PeerState::Initialized {
                                peer,
                                protocols: _,
                            } => {
                                tracing::debug!(
                                    target: LOG_TARGET,
                                    bytes_read = nread,
                                    "read message from socket"
                                );

                                match serde_cbor::from_slice(&buf[..nread]) {
                                    Ok(Message::Transaction(transaction)) => {
                                        tracing::trace!(
                                            target: LOG_TARGET,
                                            id = peer,
                                            tx = ?transaction,
                                            "received transaction from peer",
                                        );

                                        self.tx.send(PeerEvent::Message {
                                            peer,
                                            message: Message::Transaction(transaction)
                                        })
                                        .await
                                        .expect("channel to stay open");
                                    }
                                    Ok(Message::Block(block)) => {
                                        tracing::trace!(
                                            target: LOG_TARGET,
                                            id = peer,
                                            tx = ?block,
                                            "received block from peer",
                                        );

                                        self.tx.send(PeerEvent::Message {
                                            peer,
                                            message: Message::Block(block)
                                        })
                                        .await
                                        .expect("channel to stay open");
                                    }
                                    Err(err) => {
                                        tracing::error!(
                                            target: LOG_TARGET,
                                            err = ?err,
                                            "failed to decode message",
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => {
                        tracing::error!(target: LOG_TARGET, err = ?err, "falied to read from opened stream");
                    }
                },
                result = self.cmd_rx.recv() => match result {
                    Some(command) => match command {
                        PeerCommand::Disconnect => {
                            todo!();
                        }
                        PeerCommand::PublishTransaction(transaction) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                transaction = ?transaction,
                                "send transaction to peer"
                            );

                            self.socket.write(&serde_cbor::to_vec(&transaction).unwrap()).await.unwrap();
                        }
                        PeerCommand::PublishBlock(block) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                block = ?block,
                                "send block to peer"
                            );

                            self.socket.write(&serde_cbor::to_vec(&block).unwrap()).await.unwrap();
                        }
                    }
                    None => panic!("channel should stay open"),
                }
            }
        }
    }
}

// TODO: simplify this by creating the channel on `Peer` side

struct PendingInfo {
    address: SocketAddr,
    cmd_tx: Sender<PeerCommand>,
}

struct PeerInfo {
    peer: PeerId,
    cmd_tx: Sender<PeerCommand>,
    protocols: Vec<String>,
}

pub struct P2p {
    listener: TcpListener,
    cmd_rx: Receiver<Command>,
    peer_tx: Sender<PeerEvent>,
    peer_rx: Receiver<PeerEvent>,

    /// Number of peers that have connected.
    peer_count: usize,

    /// Connected peers.
    peers: HashMap<u64, PeerInfo>,

    /// Pending peers.
    pending: HashMap<usize, PendingInfo>,
}

impl P2p {
    pub fn new(listener: TcpListener, cmd_rx: Receiver<Command>) -> Self {
        let (peer_tx, peer_rx) = mpsc::channel(64);
        Self {
            listener,
            cmd_rx,
            peer_rx,
            peer_tx,
            peers: HashMap::new(),
            pending: HashMap::new(),
            peer_count: 0usize,
        }
    }

    pub async fn run(mut self) {
        loop {
            self.poll_next().await;
        }
    }

    pub async fn poll_next(&mut self) {
        tokio::select! {
            result = self.listener.accept() => match result {
                Err(err) => tracing::error!(target: LOG_TARGET, err = ?err, "failed to accept connection"),
                Ok((stream, address)) => {
                    tracing::debug!(target: LOG_TARGET, address = ?address, "node connected");

                    let tx = self.peer_tx.clone();
                    let (cmd_tx, cmd_rx) = mpsc::channel(64);
                    let peer_count = {
                        let peer_count = self.peer_count;
                        self.peer_count += 1;
                        peer_count
                    };

                    self.pending.insert(peer_count, PendingInfo {
                        address,
                        cmd_tx,
                    });

                    tokio::spawn(async move { Peer::new(peer_count, stream, tx, cmd_rx).run().await });
                }
            },
            result = self.cmd_rx.recv() => match result {
                Some(cmd) => match cmd {
                    Command::PublishBlock(block) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            block = ?block,
                            "publish block on the network",
                        );

                        todo!();
                    },
                    Command::PublishTransaction(transaction) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            transaction = ?transaction,
                            "publish transaction on the network",
                        );

                        todo!();
                    },
                    Command::DisconnectPeer(peer) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            id = ?peer,
                            "disconnect peer",
                        );

                        todo!();
                    },
                    Command::ConnectToPeer((addr, port)) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            address = ?addr,
                            port = port,
                            "attempt to connect to peer",
                        );
                    }
                }
                None => panic!("channel should stay open"),
            },
            result = self.peer_rx.recv() => match result {
                Some(event) => match event {
                    PeerEvent::Disconnected { peer } => {
                        tracing::info!(
                            target: LOG_TARGET,
                            id = peer,
                            "peer disconnected",
                        );

                        self.peers.remove(&peer);
                    },
                    PeerEvent::Connected { key, peer, protocols } => {
                        tracing::info!(
                            target: LOG_TARGET,
                            key = key,
                            id = peer,
                            protocols = ?protocols,
                            "peer connected",
                        );

                        let info = self.pending.remove(&key).expect("entry to exist");
                        self.peers.insert(peer, PeerInfo {
                            peer,
                            cmd_tx: info.cmd_tx,
                            protocols,
                        });
                    },
                    PeerEvent::Message { peer, message } => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            id = peer,
                            message = ?message,
                            "received message from peer",
                        );
                    }
                },
                None => panic!("channel should stay open"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::{fmt::format::FmtSpan, prelude::*};

    #[tokio::test]
    async fn peer_connects_and_handshakes() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
            .try_init()
            .unwrap();

        let socket = TcpListener::bind("127.0.0.1:8888").await.unwrap();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, cmd_rx);
        let mut conn = TcpStream::connect("127.0.0.1:8888");

        let (_, res2) = tokio::join!(p2p.poll_next(), conn);
        let mut stream = res2.unwrap();

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 1);
        assert_eq!(p2p.peers.len(), 0);

        let message = String::from("11,/proto/0.1.0,/proto2/1.0.0\n");
        stream.write(message.as_bytes()).await.unwrap();

        // poll the next event
        p2p.poll_next().await;

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 0);
        assert_eq!(p2p.peers.len(), 1);

        let peer = p2p.peers.get(&11).unwrap();

        assert_eq!(peer.peer, 11u64,);
        assert_eq!(
            peer.protocols,
            Vec::from(vec![
                String::from("/proto/0.1.0"),
                String::from("/proto2/1.0.0")
            ]),
        );
    }

    #[tokio::test]
    async fn peer_connects_then_disconnects() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
            .try_init()
            .unwrap();

        let socket = TcpListener::bind("127.0.0.1:8888").await.unwrap();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, cmd_rx);
        let mut conn = TcpStream::connect("127.0.0.1:8888");

        let (_, res2) = tokio::join!(p2p.poll_next(), conn);
        let mut stream = res2.unwrap();

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 1);
        assert_eq!(p2p.peers.len(), 0);

        let message = String::from("11,/proto/0.1.0,/proto2/1.0.0\n");
        stream.write(message.as_bytes()).await.unwrap();

        // poll the next event
        p2p.poll_next().await;

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 0);
        assert_eq!(p2p.peers.len(), 1);

        let peer = p2p.peers.get(&11).unwrap();

        assert_eq!(peer.peer, 11u64,);
        assert_eq!(
            peer.protocols,
            Vec::from(vec![
                String::from("/proto/0.1.0"),
                String::from("/proto2/1.0.0")
            ]),
        );

        drop(stream);

        // poll the next event
        p2p.poll_next().await;

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 0);

        // verify that the peer is no longer part of the P2P
        assert_eq!(p2p.peers.len(), 0);
    }

    #[tokio::test]
    async fn peer_sends_messages() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
            .try_init()
            .unwrap();

        let socket = TcpListener::bind("127.0.0.1:8888").await.unwrap();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, cmd_rx);
        let mut conn = TcpStream::connect("127.0.0.1:8888");

        let (_, res2) = tokio::join!(p2p.poll_next(), conn);
        let mut stream = res2.unwrap();

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 1);
        assert_eq!(p2p.peers.len(), 0);

        let message = String::from("11,/proto/0.1.0,/proto2/1.0.0");
        stream.write(message.as_bytes()).await.unwrap();

        // poll the next event
        p2p.poll_next().await;

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 0);
        assert_eq!(p2p.peers.len(), 1);

        let message =
            serde_cbor::to_vec(&Message::Transaction(Transaction::new(0, 1, 1337))).unwrap();
        tracing::warn!(target: LOG_TARGET, "send message, len {}", message.len());

        stream.write(&message).await.unwrap();

        // poll the next event
        p2p.poll_next().await;
    }
}
