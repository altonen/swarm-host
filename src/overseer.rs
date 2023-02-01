#![allow(unused)]

use crate::{
    backend::{Interface, InterfaceEvent, NetworkBackend},
    types::{OverseerEvent, DEFAULT_CHANNEL_SIZE},
};

use futures::{stream::SelectAll, FutureExt, Stream, StreamExt};
use tokio::sync::mpsc::{self, Receiver, Sender};

use std::{
    collections::{hash_map::Entry, HashMap},
    future::Future,
    net::SocketAddr,
    pin::Pin,
};

const LOG_TARGET: &'static str = "overseer";

pub struct Overseer<T: NetworkBackend> {
    /// Network-specific functionality.
    backend: T,

    /// RX channel for receiving events from RPC and peers.
    overseer_rx: Receiver<OverseerEvent<T>>,

    /// TX channel for sending events to [`Overseer`].
    overseer_tx: Sender<OverseerEvent<T>>,

    /// Handles for spawned interfaces.
    interfaces: HashMap<T::InterfaceId, T::InterfaceHandle>,

    /// Event streams for spawned interfaces.
    event_streams: SelectAll<Pin<Box<dyn Stream<Item = InterfaceEvent<T>> + Send>>>,
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

                        match self.backend.spawn_interface(address, self.overseer_tx.clone()).await {
                            Ok(handle) => match self.interfaces.entry(*handle.id()) {
                                Entry::Vacant(entry) => {
                                    self.event_streams.push_back(handle.event_stream());
                                    entry.insert(handle);
                                    // TODO: send result
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
                    OverseerEvent::Message { peer, interface, message } => {
                        todo!();
                    }
                },
                event = self.event_streams.next() => match event {
                    Some(InterfaceEvent::PeerConnected { peer, interface }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            "peer connected"
                        );

                        todo!("handle peer connected event");
                    }
                    Some(InterfaceEvent::PeerDisconnected { peer, interface }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            "peer disconnected"
                        );

                        todo!("handle peer disconnected event");
                    }
                    Some(InterfaceEvent::MessageReceived { peer, interface, message }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            "message received from peer to interface"
                        );

                        // TODO: routing table?
                        // TODO: peer id-based filtering?
                        // TODO: packet-based filtering?
                        // TODO: more complicated filtering?
                        todo!("handle message");
                    }
                    _ => {},//tracing::error!(target: LOG_TARGET, "here"),
                }
            }
        }
    }
}
