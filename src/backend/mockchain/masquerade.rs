use crate::{
    backend::{
        mockchain::{
            types::{ConnectionType, Handshake, InterfaceId, Message, PeerId, ProtocolId},
            MockchainBackend,
        },
        Interface, InterfaceEvent, InterfaceEventStream, InterfaceType, NetworkBackend, PacketSink,
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

const LOG_TARGET: &'static str = "mockchain-masquerade";

// TODO: move all type declarations to `type.rs`

#[derive(Debug)]
pub struct MockPacketSink {
    interface: InterfaceId,
    peer: PeerId,
    inner: OwnedWriteHalf,
}

impl MockPacketSink {
    pub fn new(interface: InterfaceId, peer: PeerId, inner: OwnedWriteHalf) -> Self {
        Self {
            interface,
            peer,
            inner,
        }
    }
}

#[async_trait::async_trait]
impl PacketSink<MockchainBackend> for MockPacketSink {
    async fn send_packet(
        &mut self,
        _protocol: Option<<MockchainBackend as NetworkBackend>::Protocol>,
        packet: &<MockchainBackend as NetworkBackend>::Message,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            interface_id = ?self.interface,
            peer_id = ?self.peer,
            "send packet to peer",
        );

        // TODO: proper error handling
        let packet = serde_cbor::to_vec(&packet).expect("packet to serialize");

        self.inner
            .write(&packet)
            .await
            .map(|_| ())
            .map_err(From::from)
    }

    /// Send request to peer over `protocol`.
    async fn send_request(
        &mut self,
        protocol: <MockchainBackend as NetworkBackend>::Protocol,
        request: <MockchainBackend as NetworkBackend>::Request,
    ) -> crate::Result<<MockchainBackend as NetworkBackend>::RequestId> {
        todo!();
    }

    /// Send response to peer.
    /// TODO: add request ID?
    async fn send_response(
        &mut self,
        request_id: <MockchainBackend as NetworkBackend>::RequestId,
        response: <MockchainBackend as NetworkBackend>::Response,
    ) -> crate::Result<()> {
        todo!();
    }
}

pub struct Peer;

impl Peer {
    /// Start task for a remote peer.
    // TODO: too many params? Refactor
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
                sink: Box::new(MockPacketSink::new(iface_id, handshake.peer, write)),
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

// TODO: move this code to `MockchainHandle`
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
                        tracing::debug!(target: LOG_TARGET, address = ?address, "peer connected");

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
