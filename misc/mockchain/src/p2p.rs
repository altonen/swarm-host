use crate::types::{Command, PeerId};

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

/// Events sent by the [`Peer`] to [`P2p`].
#[derive(Debug)]
enum PeerEvent {
    /// Peer disconnected.
    Disconnected { peer: PeerId },

    /// Peer connected.
    Connected {
        key: usize,
        peer: PeerId,
        protocols: Vec<String>,
    },

    Message {
        peer: PeerId,
        protocol: String,
        message: Vec<u8>,
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

    /// TX channel for sender messages.
    msg_tx: Sender<(String, String)>,

    /// RX channel for receiving messages
    msg_rx: Receiver<(String, String)>,

    /// RX channel for receiving commands.
    cmd_rx: Receiver<Command>,

    /// Peer state.
    state: PeerState,
}

impl Peer {
    pub fn new(
        key: usize,
        socket: TcpStream,
        tx: Sender<PeerEvent>,
        cmd_rx: Receiver<Command>,
    ) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel(64);

        Self {
            socket,
            tx,
            msg_tx,
            msg_rx,
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
                                    std::str::from_utf8(&buf[..nread - 1]).expect("valid utf-8 string");
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
                                peer: _,
                                protocols: _,
                            } => {
                                // TODO: fix this
                                todo!("implement Vec<u8> messages");

                                // let payload =
                                //     std::str::from_utf8(&buf[..nread - 1]).expect("valid utf-8 string");
                                // let message = payload.split(",").collect::<Vec<&str>>();
                                // let protocol = message.get(0).expect("protocol to exist").to_string();
                                // let message = message.get(1).expect("message to exist");

                                // if protocols.iter().any(|proto| proto == &protocol) {
                                //     self.tx
                                //         .send(PeerEvent::Message {
                                //             peer,
                                //             protocol,
                                //             message: message.to_string(),
                                //         })
                                //         .await
                                //         .expect("channel to stay open");
                                // } else {
                                //     tracing::warn!(target: LOG_TARGET, id = peer, protocol = protocol, "received message from uknown protocol from node");
                                // }
                            }
                        }
                    }
                    Err(err) => {
                        tracing::error!(target: LOG_TARGET, err = ?err, "falied to read from opened stream");
                    }
                },
                result = self.msg_rx.recv() => match result {
                    Some((protocol, message)) => {
                        // TODO: match on state before sending
                        tracing::trace!(target: LOG_TARGET, protocol = protocol, "send message to node");
                        self.socket.write(message.as_bytes()).await.unwrap();
                    }
                    None => panic!("channel should stay open"),
                },
                result = self.cmd_rx.recv() => match result {
                    Some(cmd) => match cmd {
                        Command::PublishTransaction(_transaction) => {
                            todo!();
                        }
                        Command::PublishBlock(_block) => {
                            todo!();
                        }
                        Command::DisconnectPeer(_peer) => {
                            todo!();
                        }
                        Command::ConnectToPeer((_addr, _port)) => {
                            todo!();
                        }
                    }
                    None => panic!("channel should stay open"),
                }
            }
        }
    }
}

struct PendingInfo {
    address: SocketAddr,
    cmd_tx: Sender<Command>,
}

struct PeerInfo {
    peer: PeerId,
    cmd_tx: Sender<Command>,
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
                    PeerEvent::Message { peer, protocol, message: _, } => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            id = peer,
                            protocol = protocol,
                            "received message from peer",
                        );
                        todo!("do something with message");
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
    async fn test_peer_connection() {
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
        assert!(p2p.peers.get(&11).is_some());
    }
}
