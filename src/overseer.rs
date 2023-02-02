#![allow(unused)]

use crate::{
    backend::{Interface, InterfaceEvent, InterfaceType, NetworkBackend},
    filter::MessageFilter,
    types::{OverseerEvent, DEFAULT_CHANNEL_SIZE},
};

use futures::{stream::SelectAll, FutureExt, Stream, StreamExt};
use tokio::{
    io::AsyncWrite,
    sync::mpsc::{self, Receiver, Sender},
};

use std::{
    collections::{hash_map::Entry, HashMap},
    future::Future,
    net::SocketAddr,
    pin::Pin,
};

const LOG_TARGET: &'static str = "overseer";

struct PeerInfo<T: NetworkBackend> {
    /// Supported protocols.
    protocols: Vec<T::ProtocolId>,

    /// Socket for sending messages to peer.
    socket: Box<dyn AsyncWrite + Send>,
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

                                    self.event_streams.push(event_stream);
                                    self.filter.register_interface(*handle.id(), InterfaceType::Masquerade);
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
                },
                event = self.event_streams.next() => match event {
                    Some(InterfaceEvent::PeerConnected { peer, interface, protocols, socket }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            "peer connected"
                        );

                        self.iface_peers.entry(interface).or_default().insert(
                            peer,
                            PeerInfo {
                                protocols,
                                socket,
                            }
                        );
                        self.filter.register_peer(peer, interface);
                    }
                    Some(InterfaceEvent::PeerDisconnected { peer, interface }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            "peer disconnected"
                        );

                        match self.iface_peers.entry(interface) {
                            Entry::Vacant(_) => tracing::error!(
                                target: LOG_TARGET,
                                peer_id = ?peer,
                                interface = ?interface,
                                "interface does not exist",
                            ),
                            Entry::Occupied(mut entry) => if entry.get_mut().remove(&peer).is_none() {
                                tracing::warn!(
                                    target: LOG_TARGET,
                                    peer_id = ?peer,
                                    interface_id = ?interface,
                                    "peer does not exist",
                                );
                            }
                        }
                    }
                    Some(InterfaceEvent::MessageReceived { peer, interface, message }) => {
                        tracing::trace!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            message = ?message,
                            "message received from peer"
                        );

                        for (interface, peer, message) in self.filter.inject_message(
                            peer,
                            interface,
                            message
                        ) {
                            todo!();
                        }
                    }
                    _ => {},
                }
            }
        }
    }
}
