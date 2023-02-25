use crate::{
    backend::{
        ConnectionUpgrade, Interface, InterfaceEvent, InterfaceEventStream, InterfaceType,
        NetworkBackend, PacketSink,
    },
    types::DEFAULT_CHANNEL_SIZE,
};

use futures::{channel, FutureExt, StreamExt};
use sc_network::{
    config::NetworkConfiguration, Command, NodeType, ProtocolName, SubstrateNetwork,
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
use sp_runtime::traits::{Block, NumberFor};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{instrument::WithSubscriber, Subscriber};

use std::{collections::HashSet, iter, net::SocketAddr, time::Duration};

// TODO: differentiate between notifications and requests?
// TODO: new protocol opened -> apply upgrade for packetsink?

#[cfg(test)]
mod tests;

const LOG_TARGET: &'static str = "substrate";

#[derive(Debug)]
pub struct SubstratePacketSink {
    peer: sc_network::PeerId,
    tx: mpsc::Sender<Command>,
}

impl SubstratePacketSink {
    pub fn new(peer: sc_network::PeerId, tx: mpsc::Sender<Command>) -> Self {
        Self { peer, tx }
    }
}

/// Abstraction which allows `swarm-host` to send packets to peer.
#[async_trait::async_trait]
impl PacketSink<SubstrateBackend> for SubstratePacketSink {
    /// Send packet to peer over `protocol`.
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
}

pub struct InterfaceHandle {
    interface_id: usize, // tx: mpsc::Sender<InterfaceEvent<SubstrateBackend>>,
                         // req_rx: channel::mpsc::Receiver<IncomingRequest>,
                         // event_rx: mpsc::Receiver<SubstrateNetworkEvent>,
}

pub enum SubstrateMessage {}

fn build_block_announce_protocol() -> NonDefaultSetConfig {
    NonDefaultSetConfig {
        notifications_protocol: format!("/sup/block-announces/1",).into(),
        fallback_names: vec![],
        max_notification_size: 8 * 1024 * 1024,
        handshake: None,
        set_config: SetConfig {
            in_peers: 0,
            out_peers: 0,
            reserved_nodes: Vec::new(),
            non_reserved_mode: NonReservedPeerMode::Deny,
        },
    }
}

fn build_request_response_protocols() -> (
    channel::mpsc::Receiver<IncomingRequest>,
    Vec<ProtocolConfig>,
) {
    let (tx, rx) = futures::channel::mpsc::channel(DEFAULT_CHANNEL_SIZE);

    // TODO: crate new tx for each request type
    (
        rx,
        vec![
            ProtocolConfig {
                name: "/sup/sync/2".into(),
                fallback_names: vec![],
                max_request_size: 1024 * 1024,
                max_response_size: 16 * 1024 * 1024,
                request_timeout: Duration::from_secs(20),
                inbound_queue: Some(tx.clone()),
            },
            ProtocolConfig {
                name: "/sup/state/2".into(),
                fallback_names: vec![],
                max_request_size: 1024 * 1024,
                max_response_size: 16 * 1024 * 1024,
                request_timeout: Duration::from_secs(40),
                inbound_queue: Some(tx.clone()),
            },
            ProtocolConfig {
                name: "/sup/sync/warp".into(),
                fallback_names: vec![],
                max_request_size: 32,
                max_response_size: 16 * 1024 * 1024,
                request_timeout: Duration::from_secs(10),
                inbound_queue: Some(tx.clone()),
            },
            ProtocolConfig {
                name: "/sup/light/2".into(),
                fallback_names: vec![],
                max_request_size: 1 * 1024 * 1024,
                max_response_size: 16 * 1024 * 1024,
                request_timeout: Duration::from_secs(15),
                inbound_queue: Some(tx),
            },
        ],
    )
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

        let node_type = match interface_type {
            InterfaceType::Masquerade => NodeType::Masquerade {
                role: Role::Full,
                block_announce_config: build_block_announce_protocol(),
            },
            InterfaceType::NodeBacked => todo!("node-backed interfaces not implemented"),
        };
        let (config, req_rx) = {
            let mut config = NetworkConfiguration::new_local();
            let (req_rx, configs) = build_request_response_protocols();

            config.request_response_protocols = configs;
            config
                .listen_addresses
                .push("/ip6/::1/tcp/8888".parse().unwrap());
            (config, req_rx)
        };

        // TODO: create all protocols on substrate side
        let mut network = SubstrateNetwork::new(
            &config,
            node_type,
            Box::new(move |fut| {
                tokio::spawn(fut);
            }),
            event_tx,
            command_rx,
        )?;
        tokio::spawn(network.run());

        // TODO: remove this and handle all messages on `substrate` side
        tokio::spawn(async move {
            let mut fused_rx = req_rx.fuse();

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
                    },
                    request = fused_rx.next() => match request.expect("channel to stay open") {
                        request => {
                            tracing::warn!(
                                target: LOG_TARGET,
                                request = ?request,
                                "received request from some peer, cannot handle requests yet"
                            );
                        },
                    }
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
    type PeerId = sc_network::PeerId;
    type InterfaceId = usize;
    type Protocol = ProtocolName;
    type Message = Vec<u8>;
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
