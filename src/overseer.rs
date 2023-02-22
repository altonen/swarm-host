#![allow(unused)]

use crate::{
    backend::{Interface, InterfaceEvent, InterfaceType, NetworkBackend, PacketSink},
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
    collections::{hash_map::Entry, HashMap},
    future::Future,
    net::SocketAddr,
    pin::Pin,
};

// TODO: get filter type from rpc
// TODO: all links should not ben bidrectional

/// Logging target for the file.
const LOG_TARGET: &'static str = "overseer";

/// Peer-related information.
struct PeerInfo<T: NetworkBackend> {
    /// Supported protocols.
    protocols: Vec<T::Protocol>,

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

impl<T: NetworkBackend> Overseer<T> {
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

                        // TODO: explain `expect()`
                        // TODO: handle both `PeerAlreadyExists` and `InterfaceDoesntExist`
                        self
                            .filter
                            .register_peer(interface, peer, FilterType::FullBypass)
                            .expect("unique (interface, peer) combination");
                        self.iface_peers.entry(interface).or_default().insert(
                            peer,
                            PeerInfo {
                                protocols,
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
                    Some(InterfaceEvent::MessageReceived { peer, interface, protocol: _, message }) => {
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
                                match self.iface_peers.get_mut(&interface).expect("interface to exist").get_mut(&peer) {
                                    None => tracing::error!(
                                        target: LOG_TARGET,
                                        interface_id = ?interface,
                                        peer_id = ?peer,
                                        "peer does not exist"
                                    ),
                                    Some(peer_info) => {
                                        match peer_info.sink.send_packet(&message).await {
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
                    _ => {},
                }
            }
        }
    }
}
