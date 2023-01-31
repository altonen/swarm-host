#![allow(unused)]

use crate::{
    backend::{Interface, InterfaceEvent, NetworkBackend},
    types::{OverseerEvent, DEFAULT_CHANNEL_SIZE},
};

use futures::{stream::FuturesOrdered, Stream, StreamExt};
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

    /// RX channel for receiving events from RPC.
    overseer_rx: Receiver<OverseerEvent>,

    /// Spawned interface handles.
    interfaces: HashMap<T::InterfaceId, T::InterfaceHandle>,

    /// Event streams for spawned interfaces.
    event_streams: FuturesOrdered<Pin<Box<dyn Future<Output = InterfaceEvent<T>>>>>,
}

impl<T: NetworkBackend> Overseer<T> {
    pub fn new(backend: T) -> Self {
        let (_tx, overseer_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        Self {
            backend,
            overseer_rx,
            interfaces: HashMap::new(),
            event_streams: FuturesOrdered::new(),
        }
    }

    pub async fn _run(mut self) {
        tracing::info!(target: LOG_TARGET, "starting overseer");

        loop {
            tokio::select! {
                result = self.overseer_rx.recv() => match result.expect("channel to stay open") {
                    OverseerEvent::CreateInterface { address, result: _ } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            address = ?address,
                            "create new interface for swarm-host",
                        );

                        match self.backend.spawn_interface(address) {
                            Ok(handle) => {
                                let event_stream = handle.event_stream();
                                let id = *handle.id();

                                match self.interfaces.entry(id) {
                                    Entry::Vacant(entry) => {
                                        entry.insert(handle);
                                        self.event_streams.push_back(event_stream);
                                    }
                                    _ => tracing::error!(
                                        target: LOG_TARGET,
                                        id = ?id,
                                        "duplicate interface id"
                                    ),
                                }
                            }
                            Err(err) => {
                                tracing::error!(
                                    target: LOG_TARGET,
                                    error = ?err,
                                    "failed to start interface"
                                );
                            }
                        }
                    }
                },
                event = self.event_streams.select_next_some() => match event {
                    InterfaceEvent::PeerConnected { peer, interface } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            "peer connected"
                        );

                        todo!("handle peer connected event");
                    }
                    InterfaceEvent::PeerDisconnected { peer, interface } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            "peer disconnected"
                        );

                        todo!("handle peer disconnected event");
                    }
                    InterfaceEvent::MessageReceived { peer, interface, message } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            peer_id = ?peer,
                            interface_id = ?interface,
                            "message received from peer to interface"
                        );

                        todo!("handle message");
                    }
                }
            }
        }
    }
}
