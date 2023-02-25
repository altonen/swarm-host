#![allow(unused)]

use crate::{
    backend::{
        ConnectionUpgrade, Interface, InterfaceEvent, InterfaceType, NetworkBackend, PacketSink,
    },
    error::Error,
    filter::{FilterType, LinkType, MessageFilter},
    types::{OverseerEvent, DEFAULT_CHANNEL_SIZE},
};

use futures::{stream::SelectAll, FutureExt, Stream, StreamExt};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    sync::mpsc::{self, Receiver, Sender},
};

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
    future::Future,
    hash::Hash,
    net::SocketAddr,
    pin::Pin,
};

// TODO: get filter type from rpc
// TODO: all links should not ben bidrectional
// TODO: split code into functions
// TODO: move tests to separate direcotry

/// Logging target for the file.
const LOG_TARGET: &'static str = "overseer";

/// Peer-related information.
struct PeerInfo<T: NetworkBackend> {
    /// Supported protocols.
    protocols: HashSet<T::Protocol>,

    /// Sink for sending messages to peer.
    sink: Box<dyn PacketSink<T> + Send>,
}

/// Object overseeing `swarm-host` execution.
pub struct Overseer<T: NetworkBackend> {
    /// Network-specific functionality.
    backend: T,

    /// RX channel for receiving events from RPC and peers.
    overseer_rx: Receiver<OverseerEvent<T>>,

    /// TX channel for sending events to [`Overseer`].
    overseer_tx: Sender<OverseerEvent<T>>,

    /// Handles for spawned interfaces.
    interfaces: HashMap<T::InterfaceId, T::InterfaceHandle>,

    /// Interface peers.
    iface_peers: HashMap<T::InterfaceId, HashMap<T::PeerId, PeerInfo<T>>>,

    /// Event streams for spawned interfaces.
    event_streams: SelectAll<Pin<Box<dyn Stream<Item = InterfaceEvent<T>> + Send>>>,

    /// Message filters.
    filter: MessageFilter<T>,
}

impl<T: NetworkBackend + Debug> Overseer<T> {
    /// Create new [`Overseer`].
    pub fn new() -> (Self, Sender<OverseerEvent<T>>) {
        let (overseer_tx, overseer_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        (
            Self {
                backend: T::new(),
                overseer_rx,
                interfaces: HashMap::new(),
                event_streams: SelectAll::new(),
                overseer_tx: overseer_tx.clone(),
                iface_peers: HashMap::new(),
                filter: MessageFilter::new(),
            },
            overseer_tx,
        )
    }

    /// Start running the [`Overseer`] event loop.
    pub async fn run(mut self) {
        tracing::info!(target: LOG_TARGET, "starting overseer");

        loop {
            tokio::select! {
                result = self.overseer_rx.recv() => match result.expect("channel to stay open") {
                    OverseerEvent::CreateInterface { address, result } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            address = ?address,
                            "create new interface",
                        );

                        match self.backend.spawn_interface(address, InterfaceType::Masquerade).await {
                            Ok((mut handle, event_stream)) => match self.interfaces.entry(*handle.id()) {
                                Entry::Vacant(entry) => {
                                    tracing::trace!(
                                        target: LOG_TARGET,
                                        "interface created"
                                    );

                                    // NOTE: it is logic error for the interface to exist in `MessageFilter`
                                    // if it doesn't exist in `self.interfaces` so `expect` is justified.
                                    self
                                        .filter
                                        .register_interface(*handle.id(), FilterType::FullBypass)
                                        .expect("unique interface");
                                    self.event_streams.push(event_stream);
                                    result.send(Ok(*handle.id())).expect("channel to stay open");
                                    entry.insert(handle);
                                },
                                Entry::Occupied(_) => tracing::error!(
                                    target: LOG_TARGET,
                                    id = ?*handle.id(),
                                    "duplicate interface id"
                                ),
                            }
                            Err(err) => {
                                tracing::error!(
                                    target: LOG_TARGET,
                                    error = ?err,
                                    "failed to start interface"
                                );
                            },
                        }
                    }
                    OverseerEvent::LinkInterface { first, second, result } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface = ?first,
                            interface = ?second,
                            "link interfaces",
                        );

                        result.send(self.filter.link_interface(first, second, LinkType::Bidrectional));
                    }
                    OverseerEvent::UnlinkInterface { first, second, result } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface = ?first,
                            interface = ?second,
                            "unlink interfaces",
                        );

                        result.send(self.filter.unlink_interface(first, second));
                    }
                    OverseerEvent::AddFilter { interface, filter_name, result } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface = ?interface,
                            filter_name = filter_name,
                            "add filter",
                        );

                        let call_result = self.interfaces.get(&interface).map_or(
                            Err(Error::InterfaceDoesntExist),
                            |iface| {
                                match iface.filter(&filter_name) {
                                    Some(filter) => self.filter.add_filter(interface, filter),
                                    None => Err(Error::FilterDoesntExist),
                                }
                            }
                        );

                        result.send(call_result).expect("channel to stay open");
                    }
                },
                event = self.event_streams.next() => match event {
                    Some(InterfaceEvent::PeerConnected { peer, interface, protocols, sink }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface_id = ?interface,
                            peer_id = ?peer,
                            "peer connected"
                        );

                        // TODO: more comprehensive error handling
                        match self
                            .filter
                            .register_peer(interface, peer, FilterType::FullBypass) {
                                Err(Error::PeerAlreadyExists) => {
                                    tracing::warn!(
                                        target: LOG_TARGET,
                                        interface_id = ?interface,
                                        peer_id = ?peer,
                                        "peer already exists in the filter",
                                    );
                                }
                                Ok(_) => {},
                                Err(err) => panic!("unrecoverable error occurred: {err:?}"),
                            }
                        self.iface_peers.entry(interface).or_default().insert(
                            peer,
                            PeerInfo {
                                protocols: HashSet::from_iter(protocols.into_iter()),
                                sink,
                            }
                        );
                    }
                    Some(InterfaceEvent::PeerDisconnected { peer, interface }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface_id = ?interface,
                            peer_id = ?peer,
                            "peer disconnected"
                        );

                        match self.iface_peers.get_mut(&interface) {
                            None => tracing::error!(
                                target: LOG_TARGET,
                                interface = ?interface,
                                peer_id = ?peer,
                                "interface does not exist",
                            ),
                            Some(peers) => if peers.remove(&peer).is_none() {
                                tracing::warn!(
                                    target: LOG_TARGET,
                                    interface_id = ?interface,
                                    peer_id = ?peer,
                                    "peer does not exist",
                                );
                            }
                        }
                    }
                    Some(InterfaceEvent::MessageReceived { peer, interface, protocol, message }) => {
                        // TODO: span?
                        tracing::trace!(
                            target: LOG_TARGET,
                            interface_id = ?interface,
                            peer_id = ?peer,
                            message = ?message,
                            "message received from peer"
                        );

                        match self.filter.inject_message(
                            interface,
                            peer,
                            &message
                        ) {
                            Ok(routing_table) => for (interface, peer) in routing_table {
                                match self
                                    .iface_peers
                                    .get_mut(&interface)
                                    .expect("interface to exist")
                                    .get_mut(&peer)
                                {
                                    None => tracing::error!(
                                        target: LOG_TARGET,
                                        interface_id = ?interface,
                                        peer_id = ?peer,
                                        "peer does not exist"
                                    ),
                                    Some(peer_info) => {
                                        match peer_info.sink.send_packet(Some(protocol.clone()), &message).await {
                                            Ok(_) =>
                                                tracing::trace!(
                                                    target: LOG_TARGET,
                                                    interface_id = ?interface,
                                                    peer_id = ?peer,
                                                    "message sent to peer",
                                                ),
                                            Err(err) => tracing::error!(
                                                target: LOG_TARGET,
                                                interface_id = ?interface,
                                                peer_id = ?peer,
                                                error = ?err,
                                                "failed to send message"
                                            ),
                                        }
                                    }
                                }
                            }
                            Err(err) => tracing::error!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                err = ?err,
                                "failed to inject message into `MessageFilter`",
                            ),
                        }
                    }
                    Some(InterfaceEvent::ConnectionUpgraded { peer, interface, upgrade }) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            interface_id = ?interface,
                            peer_id = ?peer,
                            upgrade = ?upgrade,
                            "apply upgrade to connection",
                        );

                        if let Err(err) = self.apply_connection_upgrade(interface, peer, upgrade) {
                            tracing::trace!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                error = ?err,
                                "failed to apply connection upgrade",
                            );
                        }
                    }
                    _ => {},
                }
            }
        }
    }

    /// Apply connection upgrade for an active peer.
    fn apply_connection_upgrade(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        upgrade: ConnectionUpgrade<T>,
    ) -> crate::Result<()> {
        let peer_info = self
            .iface_peers
            .get_mut(&interface)
            .expect("interface to exist")
            .get_mut(&peer)
            .ok_or(Error::PeerDoesntExist)?;

        match upgrade {
            ConnectionUpgrade::ProtocolOpened { protocols } => {
                peer_info.protocols.extend(protocols)
            }
            ConnectionUpgrade::ProtocolClosed { protocols } => peer_info
                .protocols
                .retain(|protocol| !protocols.contains(protocol)),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mockchain::{types::ProtocolId, MockchainBackend};
    use rand::Rng;

    // TODO: use `mockall`
    struct DummySink;

    #[async_trait::async_trait]
    impl PacketSink<MockchainBackend> for DummySink {
        async fn send_packet(
            &mut self,
            message: &<MockchainBackend as NetworkBackend>::Message,
        ) -> crate::Result<()> {
            todo!();
        }
    }

    #[test]
    fn apply_connection_upgrade() {
        let mut rng = rand::thread_rng();
        let (mut overseer, _) = Overseer::<MockchainBackend>::new();
        let interface = rng.gen();
        let peer = rng.gen();

        // overseer.iface_peers.insert(interface, HashMap::new());
        overseer.iface_peers.entry(interface).or_default().insert(
            peer,
            PeerInfo {
                protocols: HashSet::from([ProtocolId::Transaction, ProtocolId::Block]),
                sink: Box::new(DummySink),
            },
        );

        assert_eq!(
            overseer
                .iface_peers
                .get(&interface)
                .unwrap()
                .get(&peer)
                .unwrap()
                .protocols,
            HashSet::from([ProtocolId::Transaction, ProtocolId::Block]),
        );

        // add new protocol and verify peer protocols are updated
        overseer.apply_connection_upgrade(
            interface,
            peer,
            ConnectionUpgrade::ProtocolOpened {
                protocols: HashSet::from([ProtocolId::Generic]),
            },
        );

        assert_eq!(
            overseer
                .iface_peers
                .get(&interface)
                .unwrap()
                .get(&peer)
                .unwrap()
                .protocols,
            HashSet::from([
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::Generic
            ]),
        );

        // close two protocols: one that's supported and one that's not and verify state again
        overseer.apply_connection_upgrade(
            interface,
            peer,
            ConnectionUpgrade::ProtocolClosed {
                protocols: HashSet::from([ProtocolId::PeerExchange, ProtocolId::Transaction]),
            },
        );

        assert_eq!(
            overseer
                .iface_peers
                .get(&interface)
                .unwrap()
                .get(&peer)
                .unwrap()
                .protocols,
            HashSet::from([ProtocolId::Block, ProtocolId::Generic]),
        );
    }
}
