use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Sender, Receiver},
};

const LOG_TARGET: &'static str = "p2p";

pub type PeerId = u64;

// TODO: spawn task for each peer
//        - listen for incoming blocks and transations
//        - publish blocks and transations

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

    // /// TX channel for communication with `swarm-host`.
    // tx: Sender<types::Event>,

    /// TX channel for sender messages from `swarm-host`.
    tx_msg: Sender<(String, String)>,

    /// RX channel for receiving messages from `swarm-host`.
    rx_msg: Receiver<(String, String)>,

    /// Peer state.
    state: PeerState,
}

impl Peer {
    pub fn new(socket: TcpStream) -> Self {
        let (tx_msg, rx_msg) = mpsc::channel(64);

        Self {
            socket,
            // tx,
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

                            // self.tx
                            //     .send(types::Event::Disconnected(peer))
                            //     .await
                            //     .expect("channel to stay open");
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

                                // self.tx
                                //     .send(types::Event::Connected {
                                //         peer,
                                //         tx: self.tx_msg.clone(),
                                //         protocols: protocols.clone(),
                                //     })
                                //     .await
                                //     .expect("channel to stay open");
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
                                    // self.tx
                                    //     .send(types::Event::Message {
                                    //         peer,
                                    //         protocol,
                                    //         message: message.to_string(),
                                    //     })
                                    //     .await
                                    //     .expect("channel to stay open");
                                } else {
                                    tracing::warn!(target: LOG_TARGET, id = peer, protocol = protocol, "received message from uknown protocol from node");
                                }
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
                    None => panic!("should not fail"),
                }
            }
        }
    }
}

pub struct P2p {
    listener: TcpListener,
    // tx: Sender<types::Event>,
    // rx: Receiver<types::Event>,
}

impl P2p {
    pub fn new(
        listener: TcpListener,
        // rx: Receiver<types::Event>,
        // tx: Sender<types::Event>,
    ) -> Self {
        Self { listener }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                result = self.listener.accept() => match result {
                    Err(err) => tracing::error!(target: LOG_TARGET, err = ?err, "failed to accept connection"),
                    Ok((stream, address)) => {
                        tracing::debug!(target: LOG_TARGET, address = ?address, "node connected");

                        // let tx = self.tx.clone();
                        tokio::spawn(async move { Peer::new(stream).run().await });
                    }
                }
            }
        }
    }
}
