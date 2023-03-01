use crate::{
    backend::{
        ConnectionUpgrade, IdableRequest, Interface, InterfaceEvent, InterfaceEventStream,
        InterfaceType, NetworkBackend, PacketSink,
    },
    types::DEFAULT_CHANNEL_SIZE,
};

use futures::{channel, FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{instrument::WithSubscriber, Subscriber};

use sc_network::{
    config::NetworkConfiguration, Command, NodeType, PeerId, ProtocolName, SubstrateNetwork,
    SubstrateNetworkEvent,
};
use sc_network_common::{
    config::{
        NonDefaultSetConfig, NonReservedPeerMode, NotificationHandshake, ProtocolId, SetConfig,
    },
    protocol::role::{Role, Roles},
    request_responses::{IncomingRequest, ProtocolConfig},
    sync::message::BlockAnnouncesHandshake,
};

use std::{collections::HashSet, iter, net::SocketAddr, time::Duration};

// TODO: differentiate between notifications and requests?
// TODO: new protocol opened -> apply upgrade for packetsink?

#[cfg(test)]
mod tests;

const LOG_TARGET: &'static str = "substrate";

#[derive(Debug)]
pub struct SubstrateRequest {
    id: usize,
    payload: Vec<u8>,
}

impl IdableRequest<SubstrateBackend> for SubstrateRequest {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::RequestId {
        &self.id
    }
}

#[derive(Debug)]
pub struct SubstratePacketSink {
    peer: PeerId,
    tx: mpsc::Sender<Command>,
}

impl SubstratePacketSink {
    pub fn new(peer: PeerId, tx: mpsc::Sender<Command>) -> Self {
        Self { peer, tx }
    }
}

/// Abstraction which allows `swarm-host` to send packets to peer.
#[async_trait::async_trait]
impl PacketSink<SubstrateBackend> for SubstratePacketSink {
    async fn send_packet(
        &mut self,
        protocol: Option<<SubstrateBackend as NetworkBackend>::Protocol>,
        message: &<SubstrateBackend as NetworkBackend>::Message,
    ) -> crate::Result<()> {
        self.tx
            .send(Command::SendNotification {
                peer: self.peer,
                protocol: protocol.expect("protocol to exist"),
                message: message.to_vec(),
            })
            .await
            .expect("channel to stay open");

        Ok(())
    }

    async fn send_request(
        &mut self,
        protocol: <SubstrateBackend as NetworkBackend>::Protocol,
        message: <SubstrateBackend as NetworkBackend>::Request,
    ) -> crate::Result<<SubstrateBackend as NetworkBackend>::RequestId> {
        todo!();
    }

    async fn send_response(
        &mut self,
        request_id: <SubstrateBackend as NetworkBackend>::RequestId,
        message: <SubstrateBackend as NetworkBackend>::Response,
    ) -> crate::Result<()> {
        todo!();
    }
}

pub struct InterfaceHandle {
    interface_id: usize,
}

impl InterfaceHandle {
    // TODO: pass interface id here
    // TODO: pass listen address
    pub async fn new(
        interface_type: InterfaceType,
        interface_id: usize,
    ) -> crate::Result<(Self, InterfaceEventStream<SubstrateBackend>)> {
        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let (event_tx, mut event_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let (command_tx, mut command_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        // TODO: create all protocols on substrate side
        let mut network = SubstrateNetwork::new(
            NodeType::Masquerade,
            Box::new(move |fut| {
                tokio::spawn(fut);
            }),
            event_tx,
            command_rx,
        )?;
        tokio::spawn(network.run());

        // TODO: remove this and handle all messages on `substrate` side
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = event_rx.recv() => match event.expect("channel to stay open") {
                        SubstrateNetworkEvent::PeerConnected { peer } => {
                            tx.send(InterfaceEvent::PeerConnected {
                                peer,
                                interface: interface_id,
                                protocols: Vec::new(),
                                sink: Box::new(SubstratePacketSink::new(peer, command_tx.clone())),
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::PeerDisconnected { peer } => {
                            tx.send(InterfaceEvent::PeerDisconnected {
                                peer,
                                interface: interface_id,
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::ProtocolOpened { peer, protocol } => {
                            tx.send(InterfaceEvent::ConnectionUpgraded {
                                peer,
                                interface: interface_id,
                                upgrade: ConnectionUpgrade::ProtocolOpened {
                                    protocols: HashSet::from([protocol]),
                                }
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::ProtocolClosed { peer, protocol } => {
                            tx.send(InterfaceEvent::ConnectionUpgraded {
                                peer,
                                interface: interface_id,
                                upgrade: ConnectionUpgrade::ProtocolClosed {
                                    protocols: HashSet::from([protocol]),
                                }
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::NotificationReceived { peer, protocol, notification } => {
                            tx.send(InterfaceEvent::MessageReceived {
                                peer,
                                interface: interface_id,
                                protocol,
                                message: notification,
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::RequestReceived { peer, protocol, request } => {
                            tx.send(InterfaceEvent::RequestReceived {
                                peer,
                                interface: interface_id,
                                protocol,
                                request: SubstrateRequest {
                                    payload: request,
                                    // TODO: get request id from substrate
                                    id: 1337usize,
                                }
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::ResponseReceived {peer, protocol, response } => {
                            tx.send(InterfaceEvent::ResponseReceived {
                                peer,
                                interface: interface_id,
                                protocol,
                                response,
                            })
                            .await
                            .expect("channel to stay open");
                        }
                    },
                }
            }
        });

        Ok((Self { interface_id }, Box::pin(ReceiverStream::new(rx))))
    }
}

impl Interface<SubstrateBackend> for InterfaceHandle {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::InterfaceId {
        &self.interface_id
    }

    /// Get handle to installed filter
    fn filter(
        &self,
        filter_name: &String,
    ) -> Option<
        Box<
            dyn Fn(
                    <SubstrateBackend as NetworkBackend>::InterfaceId,
                    <SubstrateBackend as NetworkBackend>::PeerId,
                    <SubstrateBackend as NetworkBackend>::InterfaceId,
                    <SubstrateBackend as NetworkBackend>::PeerId,
                    &<SubstrateBackend as NetworkBackend>::Message,
                ) -> bool
                + Send,
        >,
    > {
        todo!();
    }

    /// Attempt to establish connection with a remote peer.
    fn connect(&mut self, address: SocketAddr) -> crate::Result<()> {
        todo!();
    }

    /// Attempt to disconnect peer from the interface.
    fn disconnect(
        &mut self,
        peer: <SubstrateBackend as NetworkBackend>::PeerId,
    ) -> crate::Result<()> {
        todo!();
    }
}

#[derive(Debug)]
pub struct SubstrateBackend {
    next_iface_id: usize,
}

impl SubstrateBackend {
    /// Create new `SubstrateBackend`.
    pub fn new() -> Self {
        Self {
            next_iface_id: 0usize,
        }
    }

    /// Allocate ID for new interface.
    pub fn next_interface_id(&mut self) -> usize {
        let iface_id = self.next_iface_id;
        self.next_iface_id += 1;
        iface_id
    }
}

#[async_trait::async_trait]
impl NetworkBackend for SubstrateBackend {
    type PeerId = PeerId;
    type InterfaceId = usize;
    type RequestId = usize; // TODO: get from substrate
    type Protocol = ProtocolName;
    type Message = Vec<u8>;
    type Request = SubstrateRequest;
    type Response = Vec<u8>;
    type InterfaceHandle = InterfaceHandle;

    /// Create new [`SubstrateBackend`].
    fn new() -> Self {
        tracing::debug!(target: LOG_TARGET, "create new substrate backend",);

        SubstrateBackend::new()
    }

    /// Start new interface for accepting incoming connections.
    ///
    /// Return a handle which allows performing actions on the interface
    /// such as publishing messages or managing peer connections and
    /// a stream which allows reading events from interface.
    async fn spawn_interface(
        &mut self,
        address: SocketAddr,
        interface_type: InterfaceType,
    ) -> crate::Result<(Self::InterfaceHandle, InterfaceEventStream<Self>)>
    where
        Self: Sized,
    {
        tracing::debug!(
            target: LOG_TARGET,
            address = ?address,
            interface_type = ?interface_type,
            "create new substrate backend",
        );

        InterfaceHandle::new(interface_type, self.next_interface_id()).await
    }
}
