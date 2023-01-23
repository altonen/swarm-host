use crate::types::{Command, PeerId};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
};

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

    /// TX channel for communication.
    tx: Sender<PeerEvent>,

    /// TX channel for sender messages.
    tx_msg: Sender<(String, String)>,

    /// RX channel for receiving messages
    rx_msg: Receiver<(String, String)>,

    /// Peer state.
    state: PeerState,
}

impl Peer {
    pub fn new(socket: TcpStream, tx: Sender<PeerEvent>) -> Self {
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

                                tracing::debug!(target: LOG_TARGET, id = peer, protocols = ?protocols, "node connected");

                                self.tx
                                    .send(PeerEvent::Connected {
                                        peer,
                                        protocols: protocols.clone(),
                                    })
                                    .await
                                    .expect("channel to stay open");
                                self.state = PeerState::Initialized { peer, protocols };
                            }
                            PeerState::Initialized {
                                peer: _,
                                ref _protocols: _,
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
                result = self.rx_msg.recv() => match result {
                    Some((protocol, message)) => {
                        // TODO: match on state before sending
                        tracing::trace!(target: LOG_TARGET, protocol = protocol, "send message to node");
                        self.socket.write(message.as_bytes()).await.unwrap();
                    }
                    None => panic!("channel should stay open"),
                },
                // result = self.rx_cmd.recv() => match result {
                //     Some(cmd) => match cmd {
                //     }
                //     None => panic!("channel should stay open"),
                // }
            }
        }
    }
}

pub struct P2p {
    listener: TcpListener,
    cmd_rx: Receiver<Command>,
    peer_tx: Sender<PeerEvent>,
    peer_rx: Receiver<PeerEvent>,
}

impl P2p {
    pub fn new(listener: TcpListener, cmd_rx: Receiver<Command>) -> Self {
        let (peer_tx, peer_rx) = mpsc::channel(64);
        Self {
            listener,
            cmd_rx,
            peer_rx,
            peer_tx,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                result = self.listener.accept() => match result {
                    Err(err) => tracing::error!(target: LOG_TARGET, err = ?err, "failed to accept connection"),
                    Ok((stream, address)) => {
                        tracing::debug!(target: LOG_TARGET, address = ?address, "node connected");

                        let tx = self.peer_tx.clone();
                        tokio::spawn(async move { Peer::new(stream, tx).run().await });
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
                    Some(_message) => todo!("decide on format"),
                    None => panic!("channel should stay open"),
                }
            }
        }
    }
}
