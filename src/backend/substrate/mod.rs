use crate::{
    backend::{Interface, InterfaceEvent, InterfaceEventStream, InterfaceType, NetworkBackend},
    types::DEFAULT_CHANNEL_SIZE,
};

use sc_network::{config::NetworkConfiguration, NodeType, SubstrateNetwork};
use sc_network_common::{
    config::{
        NonDefaultSetConfig, NonReservedPeerMode, NotificationHandshake,
        ProtocolId as SubstrateProtocolId, SetConfig,
    },
    protocol::role::{Role, Roles},
    sync::message::BlockAnnouncesHandshake,
};
use sp_runtime::traits::{Block, NumberFor};
use std::{iter, net::SocketAddr};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[cfg(test)]
mod tests;

const LOG_TARGET: &'static str = "substrate";

#[derive(Debug, Copy, Clone)]
pub enum ProtocolId {}

pub struct InterfaceHandle {
    tx: mpsc::Sender<InterfaceEvent<SubstrateBackend>>,
}

impl InterfaceHandle {
    pub async fn new(
        interface_type: InterfaceType,
    ) -> crate::Result<(Self, InterfaceEventStream<SubstrateBackend>)> {
        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        let node_type = match interface_type {
            InterfaceType::Masquerade => NodeType::Masquerade {
                role: Role::Full,
                block_announce_config: NonDefaultSetConfig {
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
                },
            },
            InterfaceType::NodeBacked => todo!("node-backed interfaces not implemented"),
        };
        let config = {
            let mut config = NetworkConfiguration::new_local();
            config
                .listen_addresses
                .push("/ip6/::1/tcp/8888".parse().unwrap());
            config
        };

        // TODO: pass tx here?
        // TODO: return some service handle from `SubstrateNetwork` and save it in `InterfaceHandle`
        let mut network = SubstrateNetwork::new(
            &config,
            node_type,
            Box::new(move |fut| {
                tokio::spawn(fut);
            }),
        )?;
        tokio::spawn(network.run());

        Ok((Self { tx }, Box::pin(ReceiverStream::new(rx))))
    }
}

impl Interface<SubstrateBackend> for InterfaceHandle {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::InterfaceId {
        todo!();
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

pub struct SubstrateBackend {}

impl SubstrateBackend {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl NetworkBackend for SubstrateBackend {
    type PeerId = sc_network::PeerId;
    type InterfaceId = usize;
    type ProtocolId = ProtocolId;
    type Message = ();
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

        InterfaceHandle::new(interface_type).await
    }
}
