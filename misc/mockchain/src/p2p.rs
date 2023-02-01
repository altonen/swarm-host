use crate::types::{
    Block, Command, Message, MessageId, OverseerEvent, PeerId, Subsystem, Transaction,
};

use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
};

use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::Hasher,
    net::SocketAddr,
};

const LOG_TARGET: &'static str = "p2p";

// TODO: create handshake message instead of sending a comma-separated string

enum ConnectionType {
    Inbound,
    Outbound,
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

    /// Publish message on the network.
    PublishMessage(Message),
}

/// Events sent by the [`Peer`] to [`P2p`].
#[derive(Debug)]
enum PeerEvent {
    /// Peer disconnected.
    Disconnected { peer: PeerId },

    /// Peer connected.
    Connected {
        key: usize,
        peer: PeerId,
        protocols: Vec<ProtocolId>,
    },

    Message {
        peer: PeerId,
        message: Message,
        digest: u64,
    },
}

/// Supported protocols.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
enum ProtocolId {
    /// Transaction protocol.
    Transaction,

    /// Block protocol.
    Block,

    /// Peer exchange protocol.
    PeerExchange,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Handshake {
    /// Unique ID of the peer.
    peer: PeerId,

    /// Supported protocols of the peer.
    protocols: Vec<ProtocolId>,
}

// TODO: document
#[derive(Debug)]
enum PeerState {
    // TODO: document
    Uninitialized {
        key: usize,
    },

    // TODO: document
    HandshakeSent {
        key: usize,
    },

    // TODO: document
    Initialized {
        /// Peer ID.
        peer: PeerId,

        /// Supported protocols.
        protocols: Vec<ProtocolId>,
    },
}

#[derive(Debug)]
struct Peer {
    /// PeerId of [`P2p`].
    id: PeerId,

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
    pub async fn new(
        id: PeerId,
        key: usize,
        mut socket: TcpStream,
        tx: Sender<PeerEvent>,
        cmd_rx: Receiver<PeerCommand>,
        connection_type: ConnectionType,
    ) -> Self {
        let state = match connection_type {
            ConnectionType::Inbound => PeerState::Uninitialized { key },
            ConnectionType::Outbound => {
                let handshake = serde_cbor::to_vec(&Handshake {
                    peer: id,
                    protocols: vec![
                        ProtocolId::Transaction,
                        ProtocolId::Block,
                        ProtocolId::PeerExchange,
                    ],
                })
                .expect("encode to succeed");

                socket.write(&handshake).await.expect("stream to stay open");
                PeerState::HandshakeSent { key }
            }
        };

        Self {
            socket,
            id,
            tx,
            cmd_rx,
            state,
        }
    }

    pub async fn run(mut self) {
        let mut buf = vec![0u8; 1024];

        'outer: loop {
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
                            PeerState::Uninitialized { key } | PeerState::HandshakeSent { key } => {
                                let handshake: Handshake = serde_cbor::from_slice(&buf[..nread]).expect("valid handshake");
                                let peer = handshake.peer;
                                let protocols = handshake.protocols;

                                tracing::debug!(target: LOG_TARGET, id = peer, protocols = ?protocols, "node connected");

                                if std::matches!(self.state, PeerState::Uninitialized { .. }) {
                                    let handshake = serde_cbor::to_vec(&Handshake {
                                        peer: self.id,
                                        protocols: vec![
                                            ProtocolId::Transaction,
                                            ProtocolId::Block,
                                            ProtocolId::PeerExchange,
                                        ],
                                    })
                                    .expect("encode to succeed");

                                    self.socket.write(&handshake).await.expect("stream to stay open");
                                    // let handshake = format!("{},/tx/1.0.0,/block/1.0.0", self.id);
                                    // self.socket
                                    //     .write(handshake.as_bytes())
                                    //     .await
                                    //     .expect("stream to stay open");
                                }

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

                                        let digest = {
                                            let mut hasher = DefaultHasher::new();
                                            hasher.write(&buf[..nread]);
                                            hasher.finish()
                                        };

                                        self.tx.send(PeerEvent::Message {
                                            peer,
                                            message: Message::Transaction(transaction),
                                            digest,
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

                                        let digest = {
                                            let mut hasher = DefaultHasher::new();
                                            hasher.write(&buf[..nread]);
                                            hasher.finish()
                                        };

                                        self.tx.send(PeerEvent::Message {
                                            peer,
                                            message: Message::Block(block),
                                            digest,
                                        })
                                        .await
                                        .expect("channel to stay open");
                                    }
                                    Ok(message) => {
                                        let digest = {
                                            let mut hasher = DefaultHasher::new();
                                            hasher.write(&buf[..nread]);
                                            hasher.finish()
                                        };

                                        self.tx.send(PeerEvent::Message {
                                            peer,
                                            message,
                                            digest,
                                        })
                                        .await
                                        .expect("channel to stay open");
                                    },
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
                        tracing::error!(target: LOG_TARGET, err = ?err, "failed to read from opened stream");
                    }
                },
                result = self.cmd_rx.recv() => match result {
                    Some(command) => match command {
                        PeerCommand::Disconnect => {
                            tracing::trace!(target: LOG_TARGET, "disconnect peer");
                            break 'outer;
                        }
                        PeerCommand::PublishTransaction(transaction) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                transaction = ?transaction,
                                "send transaction to peer"
                            );

                            self.socket.write(&serde_cbor::to_vec(&Message::Transaction(
                                transaction
                            )).unwrap()).await.unwrap();
                        }
                        PeerCommand::PublishBlock(block) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                block = ?block,
                                "send block to peer"
                            );

                            self.socket.write(&serde_cbor::to_vec(&Message::Block(
                                block
                            )).unwrap()).await.unwrap();
                        }
                        PeerCommand::PublishMessage(message) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                message = ?message,
                                "send message to peer"
                            );

                            self.socket.write(&serde_cbor::to_vec(&message).unwrap()).await.unwrap();
                        }
                    }
                    None => panic!("channel should stay open"),
                }
            }
        }

        tracing::info!(target: LOG_TARGET, "peer existing");
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
    protocols: Vec<ProtocolId>,
}

pub struct P2p {
    id: PeerId,
    listener: TcpListener,
    address: String,
    cmd_rx: Receiver<Command>,
    peer_tx: Sender<PeerEvent>,
    peer_rx: Receiver<PeerEvent>,
    overseer_tx: Sender<OverseerEvent>,

    /// Number of peers that have connected.
    peer_count: usize,

    /// Connected peers.
    peers: HashMap<u64, PeerInfo>,

    /// Pending peers.
    pending: HashMap<usize, PendingInfo>,

    /// Seen messages.
    seen: HashMap<MessageId, HashSet<PeerId>>,
}

impl P2p {
    pub fn new(
        listener: TcpListener,
        address: String,
        cmd_rx: Receiver<Command>,
        overseer_tx: Sender<OverseerEvent>,
    ) -> Self {
        let (peer_tx, peer_rx) = mpsc::channel(64);
        let id = rand::thread_rng().gen::<PeerId>();

        tracing::info!(
            target: LOG_TARGET,
            id = ?id,
            "starting p2p"
        );

        Self {
            id,
            listener,
            address,
            cmd_rx,
            peer_rx,
            peer_tx,
            overseer_tx,
            peers: HashMap::new(),
            pending: HashMap::new(),
            seen: HashMap::new(),
            peer_count: 0usize,
        }
    }

    pub async fn run(mut self) {
        loop {
            self.poll_next().await;
        }
    }

    fn spawn_peer(
        &mut self,
        stream: TcpStream,
        address: SocketAddr,
        connection_type: ConnectionType,
    ) {
        let tx = self.peer_tx.clone();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let id = self.id;
        let peer_count = {
            let peer_count = self.peer_count;
            self.peer_count += 1;
            peer_count
        };

        self.pending
            .insert(peer_count, PendingInfo { address, cmd_tx });

        tokio::spawn(async move {
            Peer::new(id, peer_count, stream, tx, cmd_rx, connection_type)
                .await
                .run()
                .await
        });
    }

    pub async fn poll_next(&mut self) {
        tokio::select! {
            result = self.listener.accept() => match result {
                Err(err) => tracing::error!(target: LOG_TARGET, err = ?err, "failed to accept connection"),
                Ok((stream, address)) => {
                    tracing::debug!(target: LOG_TARGET, address = ?address, "node connected");
                    self.spawn_peer(stream, address, ConnectionType::Inbound);
                }
            },
            result = self.cmd_rx.recv() => match result {
                Some(cmd) => match cmd {
                    Command::PublishTransaction(transaction) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            transaction = ?transaction,
                            "publish transaction on the network",
                        );

                        for (_id, peer) in &self.peers {
                            peer.cmd_tx.send(PeerCommand::PublishTransaction(
                                transaction.clone()
                            )).await.expect("channel to stay open");
                        }
                    },
                    Command::PublishBlock(block) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            block = ?block,
                            "publish block on the network",
                        );

                        for (_id, peer) in &self.peers {
                            peer.cmd_tx.send(PeerCommand::PublishBlock(
                                block.clone()
                            )).await.expect("channel to stay open");
                        }
                    },
                    Command::DisconnectPeer(peer) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            id = ?peer,
                            "disconnect peer",
                        );

                        if let Some(info) = self.peers.remove(&peer) {
                            info.cmd_tx.send(PeerCommand::Disconnect).await.expect("channel to stay open");
                        } else {
                            tracing::error!(
                                target: LOG_TARGET,
                                id = ?peer,
                                "peer doesn't exist",
                            );
                        }
                    },
                    Command::ConnectToPeer(address, tx) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            address = ?address,
                            "attempt to connect to peer",
                        );

                        // TODO: verify that `address:port` is not self
                        let address: SocketAddr = address.parse().unwrap();

                        match tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            TcpStream::connect(address),
                        ).await {
                            Ok(result) => match result {
                                Ok(stream) => {
                                    tracing::debug!(
                                        target: LOG_TARGET,
                                        "connection established with remote peer",
                                    );

                                    tx.send(Ok(())).expect("channel to stay open");
                                    self.spawn_peer(stream, address, ConnectionType::Outbound);
                                }
                                Err(err) => {
                                    tracing::error!(
                                        target: LOG_TARGET,
                                        err = ?err,
                                        "failed to establish connection with remote peer",
                                    );
                                    tx.send(Err(String::from("connection refused"))).expect("channel to stay open");
                                }
                            }
                            Err(err) => {
                                tracing::error!(
                                    target: LOG_TARGET,
                                    err = ?err,
                                    "failed to establish connection with remote peer",
                                );
                                tx.send(Err(String::from("timeout"))).expect("channel to stay open");
                            }
                        }
                    }
                    Command::PublishMessage(message) => {
                        for (_id, peer) in &self.peers {
                            peer.cmd_tx.send(PeerCommand::PublishMessage(
                                message.clone(),
                            )).await.expect("channel to stay open");
                        }
                    }
                    Command::GetLocalPeerId(tx) => {
                        tx.send(self.id).expect("channel to stay open");
                    }
                    Command::GetLocalAddress(tx) => {
                        tx.send(self.address.clone()).expect("channel to stay open");
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
                    PeerEvent::Message { peer, message, digest } => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            id = peer,
                            message = ?message,
                            "received message from peer",
                        );

                        let peers = self.seen.entry(digest).or_insert(HashSet::new());
                        peers.insert(peer);

                        for (id, info) in &self.peers {
                            if !peers.contains(id) {
                                info.cmd_tx.send(PeerCommand::PublishMessage(
                                    message.clone(),
                                )).await.expect("channel to stay open");
                            }
                        }
                        self.overseer_tx.send(OverseerEvent::Message(Subsystem::P2p, message)).await.unwrap();
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
    use tokio::sync::oneshot;
    use tracing_subscriber::{fmt::format::FmtSpan, prelude::*};

    #[tokio::test]
    async fn peer_connects_and_handshakes() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
            .try_init()
            .unwrap();

        let socket = TcpListener::bind("127.0.0.1:8888").await.unwrap();
        let (_cmd_tx, cmd_rx) = mpsc::channel(64);
        let (overseer_tx, _overseer_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, "127.0.0.1:8888".to_string(), cmd_rx, overseer_tx);
        let conn = TcpStream::connect("127.0.0.1:8888");

        let (_, res2) = tokio::join!(p2p.poll_next(), conn);
        let mut stream = res2.unwrap();

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 1);
        assert_eq!(p2p.peers.len(), 0);

        let handshake = serde_cbor::to_vec(&Handshake {
            peer: 11u64,
            protocols: vec![
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
            ],
        })
        .expect("encode to succeed");

        stream.write(&handshake).await.expect("stream to stay open");

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
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
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
        let (_cmd_tx, cmd_rx) = mpsc::channel(64);
        let (overseer_tx, _overseer_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, "127.0.0.1:8888".to_string(), cmd_rx, overseer_tx);
        let conn = TcpStream::connect("127.0.0.1:8888");

        let (_, res2) = tokio::join!(p2p.poll_next(), conn);
        let mut stream = res2.unwrap();

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 1);
        assert_eq!(p2p.peers.len(), 0);

        let handshake = serde_cbor::to_vec(&Handshake {
            peer: 11u64,
            protocols: vec![
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
            ],
        })
        .expect("encode to succeed");

        stream.write(&handshake).await.expect("stream to stay open");

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
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
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
        let (_cmd_tx, cmd_rx) = mpsc::channel(64);
        let (overseer_tx, mut overseer_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, "127.0.0.1:8888".to_string(), cmd_rx, overseer_tx);
        let conn = TcpStream::connect("127.0.0.1:8888");

        let (_, res2) = tokio::join!(p2p.poll_next(), conn);
        let mut stream = res2.unwrap();

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 1);
        assert_eq!(p2p.peers.len(), 0);

        let handshake = serde_cbor::to_vec(&Handshake {
            peer: 11u64,
            protocols: vec![
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
            ],
        })
        .expect("encode to succeed");

        stream.write(&handshake).await.expect("stream to stay open");

        // poll the next event
        p2p.poll_next().await;

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 0);
        assert_eq!(p2p.peers.len(), 1);

        stream
            .write(
                &serde_cbor::to_vec(&Message::Transaction(Transaction::new(0, 1, 1337))).unwrap(),
            )
            .await
            .unwrap();

        // poll the next event
        p2p.poll_next().await;

        assert_eq!(
            overseer_rx.try_recv(),
            Ok(OverseerEvent::Message(
                Subsystem::P2p,
                Message::Transaction(Transaction::new(0, 1, 1337))
            )),
        );
    }

    #[tokio::test]
    async fn publish_block() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
            .try_init()
            .unwrap();

        let socket = TcpListener::bind("127.0.0.1:8888").await.unwrap();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (overseer_tx, _overseer_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, "127.0.0.1:8888".to_string(), cmd_rx, overseer_tx);
        let conn = TcpStream::connect("127.0.0.1:8888");

        let (_, res2) = tokio::join!(p2p.poll_next(), conn);
        let mut stream = res2.unwrap();

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 1);
        assert_eq!(p2p.peers.len(), 0);

        let handshake = serde_cbor::to_vec(&Handshake {
            peer: 11u64,
            protocols: vec![
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
            ],
        })
        .expect("encode to succeed");

        stream.write(&handshake).await.expect("stream to stay open");

        // poll the next event
        p2p.poll_next().await;

        let mut buf = vec![0u8; 1024];
        match stream.read(&mut buf).await {
            Err(_) => panic!("should not fail"),
            Ok(nread) => {
                let _message: Handshake = serde_cbor::from_slice(&buf[..nread]).unwrap();
                // TODO: check handshake?
            }
        }

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 0);
        assert_eq!(p2p.peers.len(), 1);

        cmd_tx
            .send(Command::PublishBlock(Block::from_transactions(vec![
                Transaction::new(1, 2, 1338),
            ])))
            .await
            .unwrap();

        // poll the next event
        p2p.poll_next().await;

        match stream.read(&mut buf).await {
            Err(_) => panic!("should not fail"),
            Ok(nread) => {
                println!("{nread}");
                let message: Message = serde_cbor::from_slice(&buf[..nread]).unwrap();

                assert_eq!(
                    message,
                    Message::Block(Block::from_transactions(vec![Transaction::new(1, 2, 1338)])),
                );
            }
        }
    }

    #[tokio::test]
    async fn publish_tx_two_peers() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
            .try_init()
            .unwrap();

        let socket = TcpListener::bind("127.0.0.1:8888").await.unwrap();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (overseer_tx, _overseer_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, "127.0.0.1:8888".to_string(), cmd_rx, overseer_tx);

        let conn = TcpStream::connect("127.0.0.1:8888");
        let (_, res1) = tokio::join!(p2p.poll_next(), conn);

        let conn = TcpStream::connect("127.0.0.1:8888");
        let (_, res2) = tokio::join!(p2p.poll_next(), conn);

        let (mut stream1, mut stream2) = (res1.unwrap(), res2.unwrap());

        assert_eq!(p2p.peer_count, 2);
        assert_eq!(p2p.pending.len(), 2);
        assert_eq!(p2p.peers.len(), 0);

        let handshake1 = serde_cbor::to_vec(&Handshake {
            peer: 11u64,
            protocols: vec![
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
            ],
        })
        .expect("encode to succeed");

        let handshake2 = serde_cbor::to_vec(&Handshake {
            peer: 22u64,
            protocols: vec![
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
            ],
        })
        .expect("encode to succeed");

        stream1.write(&handshake1).await.unwrap();
        stream2.write(&handshake2).await.unwrap();

        // poll two next events
        p2p.poll_next().await;
        p2p.poll_next().await;

        let mut buf = vec![0u8; 1024];
        match stream1.read(&mut buf).await {
            Err(_) => panic!("should not fail"),
            Ok(nread) => {
                let _message: Handshake = serde_cbor::from_slice(&buf[..nread]).unwrap();
                // TODO: check handshake?
            }
        }
        let mut buf = vec![0u8; 1024];
        match stream2.read(&mut buf).await {
            Err(_) => panic!("should not fail"),
            Ok(nread) => {
                let _message: Handshake = serde_cbor::from_slice(&buf[..nread]).unwrap();
                // TODO: check handshake?
            }
        }

        assert_eq!(p2p.peer_count, 2);
        assert_eq!(p2p.pending.len(), 0);
        assert_eq!(p2p.peers.len(), 2);

        cmd_tx
            .send(Command::PublishBlock(Block::from_transactions(vec![
                Transaction::new(13, 37, 1337),
            ])))
            .await
            .unwrap();

        // poll the next event
        p2p.poll_next().await;

        // first peer reads the transaction
        let mut buf = vec![0u8; 1024];
        match stream1.read(&mut buf).await {
            Err(_) => panic!("should not fail"),
            Ok(nread) => {
                let message: Message = serde_cbor::from_slice(&buf[..nread]).unwrap();

                assert_eq!(
                    message,
                    Message::Block(Block::from_transactions(vec![Transaction::new(
                        13, 37, 1337
                    )])),
                )
            }
        }

        match stream2.read(&mut buf).await {
            Err(_) => panic!("should not fail"),
            Ok(nread) => {
                let message: Message = serde_cbor::from_slice(&buf[..nread]).unwrap();

                assert_eq!(
                    message,
                    Message::Block(Block::from_transactions(vec![Transaction::new(
                        13, 37, 1337
                    )])),
                )
            }
        }
    }

    #[tokio::test]
    async fn outbound_peer() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
            .try_init()
            .unwrap();

        let socket = TcpListener::bind("127.0.0.1:8888").await.unwrap();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (overseer_tx, _overseer_rx) = mpsc::channel(64);
        let mut p2p = P2p::new(socket, "127.0.0.1:8888".to_string(), cmd_rx, overseer_tx);
        let inbound = TcpListener::bind("127.0.0.1:9999").await.unwrap();

        let (tx, _rx) = oneshot::channel();
        cmd_tx
            .send(Command::ConnectToPeer(String::from("127.0.0.1:9999"), tx))
            .await
            .unwrap();
        p2p.poll_next().await;

        let (mut stream, _) = inbound.accept().await.unwrap();

        let mut buf = vec![0u8; 1024];
        match stream.read(&mut buf).await {
            Err(_) => panic!("should not fail"),
            Ok(nread) => {
                assert_eq!(
                    serde_cbor::from_slice::<Handshake>(&buf[..nread]).unwrap(),
                    Handshake {
                        peer: p2p.id,
                        protocols: vec![
                            ProtocolId::Transaction,
                            ProtocolId::Block,
                            ProtocolId::PeerExchange,
                        ],
                    },
                );
            }
        }

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 1);
        assert_eq!(p2p.peers.len(), 0);

        let handshake = serde_cbor::to_vec(&Handshake {
            peer: 11u64,
            protocols: vec![
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
            ],
        })
        .expect("encode to succeed");

        stream.write(&handshake).await.expect("stream to stay open");

        // poll the next event
        p2p.poll_next().await;

        assert_eq!(p2p.peer_count, 1);
        assert_eq!(p2p.pending.len(), 0);
        assert_eq!(p2p.peers.len(), 1);

        let peer = p2p.peers.get(&11).unwrap();

        assert_eq!(peer.peer, 11u64);
        assert_eq!(
            peer.protocols,
            Vec::from(vec![
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::PeerExchange,
            ]),
        );
    }
}
