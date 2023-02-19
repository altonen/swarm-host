use crate::{
    backend::{
        mockchain::{
            types::{ConnectionType, Handshake, InterfaceId, Message, PeerId, ProtocolId},
            MockchainBackend,
        },
        Interface, InterfaceEvent, InterfaceEventStream, InterfaceType, NetworkBackend,
    },
    ensure,
    error::Error,
    types::{OverseerEvent, DEFAULT_CHANNEL_SIZE},
};

use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::mpsc::{self, Receiver, Sender},
};
use tokio_stream::wrappers::ReceiverStream;

const LOG_TARGET: &'static str = "mockchain-node-backed";

pub struct Peer;

impl Peer {
    /// Start task for a remote peer.
    pub async fn start(
        iface_tx: Sender<InterfaceEvent<MockchainBackend>>,
        mut stream: TcpStream,
        connection_type: ConnectionType,
        iface_id: InterfaceId,
    ) -> crate::Result<()> {
        let mut buf = vec![0u8; 8 * 1024];

        if let ConnectionType::Inbound = connection_type {
            let handshake = serde_cbor::to_vec(&Handshake {
                peer: 0u64,
                protocols: vec![
                    ProtocolId::Transaction,
                    ProtocolId::Block,
                    ProtocolId::PeerExchange,
                ],
            })?;

            stream.write(&handshake).await?;
        }

        let nread = stream.read(&mut buf).await?;
        let handshake: Handshake = serde_cbor::from_slice(&buf[..nread])?;

        tracing::debug!(
            target: LOG_TARGET,
            handshake = ?handshake,
            "received handshake from peer"
        );

        // TODO: verify that the peers agree on at least one protocol
        let (mut read, write) = stream.into_split();

        // TODO: use `expect()` when the leaky abstraction of `socket` is fixed
        if iface_tx
            .send(InterfaceEvent::PeerConnected {
                peer: handshake.peer,
                interface: iface_id,
                protocols: handshake.protocols,
                socket: Box::new(write),
            })
            .await
            .is_err()
        {
            panic!("essential task shut down");
        }

        loop {
            let nread = read.read(&mut buf).await?;

            if nread == 0 {
                tracing::debug!(
                    target: LOG_TARGET,
                    peer = handshake.peer,
                    interface = ?iface_id,
                    "connection closed to peer",
                );

                // TODO: use `expect()` when the leaky abstraction of `socket` is fixed
                if iface_tx
                    .send(InterfaceEvent::PeerDisconnected {
                        peer: handshake.peer,
                        interface: iface_id,
                    })
                    .await
                    .is_err()
                {
                    panic!("essential task shut down");
                }

                return Ok(());
            }

            match serde_cbor::from_slice::<Message>(&buf[..nread]) {
                Ok(message) => {
                    // TODO: use `expect()` when the leaky abstraction of `socket` is fixed
                    if iface_tx
                        .send(InterfaceEvent::MessageReceived {
                            peer: handshake.peer,
                            interface: iface_id,
                            protocol: ProtocolId::Generic,
                            message,
                        })
                        .await
                        .is_err()
                    {
                        panic!("essential task shut down");
                    }
                }
                Err(err) => tracing::warn!(
                    target: LOG_TARGET,
                    peer = handshake.peer,
                    interface = ?iface_id,
                    err = ?err,
                    "peer send an invalid message",
                ),
            }
        }
    }
}

// TODO: how node-backed interface works:
//        - special type of connection listener:
//        	- first received connection is the node that backs the interfae
//        	- interface copies the handshake of this interface
//        	- subsequent connections are fed this handshake

pub struct P2p;

impl P2p {
    /// Start the P2P functionality.
    pub fn start(
        iface_tx: Sender<InterfaceEvent<MockchainBackend>>,
        listener: TcpListener,
        iface_id: InterfaceId,
    ) -> Self {
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Err(err) => tracing::error!(
                        target: LOG_TARGET,
                        err = ?err,
                        "failed to accept connection"
                    ),
                    Ok((stream, address)) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            address = ?address,
                            "peer connected",
                        );

                        let iface_tx_copy = iface_tx.clone();
                        tokio::spawn(async move {
                            if let Err(err) = Peer::start(
                                iface_tx_copy,
                                stream,
                                ConnectionType::Inbound,
                                iface_id,
                            )
                            .await
                            {
                                tracing::error!(
                                    target: LOG_TARGET,
                                    err = ?err,
                                    "failed to handle peer connection",
                                );
                            }
                        });
                    }
                }
            }
        });

        Self {}
    }
}
