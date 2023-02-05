#![allow(unused)]

use crate::{
    backend::{Interface, InterfaceEvent, InterfaceType, NetworkBackend},
    filter::{FilterType, MessageFilter},
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

const LOG_TARGET: &'static str = "overseer";

struct PeerInfo<T: NetworkBackend> {
    /// Supported protocols.
    protocols: Vec<T::ProtocolId>,

    /// Socket for sending messages to peer.
    socket: Box<dyn AsyncWrite + Send + Unpin>,
}

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

                        match self.backend.spawn_interface(address).await {
                            Ok((mut handle, event_stream)) => match self.interfaces.entry(*handle.id()) {
                                Entry::Vacant(entry) => {
                                    tracing::trace!(
                                        target: LOG_TARGET,
                                        "interface created"
                                    );

                                    // it is logic error for the interface to exist in `MessageFilter`
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

                        result.send(self.filter.link_interface(first, second));
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
                },
                event = self.event_streams.next() => match event {
                    Some(InterfaceEvent::PeerConnected { peer, interface, protocols, socket }) => {
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
                                socket,
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

                        match self.iface_peers.entry(interface) {
                            Entry::Vacant(_) => tracing::error!(
                                target: LOG_TARGET,
                                interface = ?interface,
                                peer_id = ?peer,
                                "interface does not exist",
                            ),
                            Entry::Occupied(mut entry) => if entry.get_mut().remove(&peer).is_none() {
                                tracing::warn!(
                                    target: LOG_TARGET,
                                    interface_id = ?interface,
                                    peer_id = ?peer,
                                    "peer does not exist",
                                );
                            }
                        }
                    }
                    Some(InterfaceEvent::MessageReceived { peer, interface, message }) => {
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
                                        "peer doesn't exist"
                                    ),
                                    Some(peer_info) => {
                                        let message = serde_cbor::to_vec(&message).expect("message to serialize");
                                        match peer_info.socket.write(&message).await {
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
