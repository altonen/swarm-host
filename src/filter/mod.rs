use crate::{
    backend::{Idable, NetworkBackend, PacketSink},
    error::{Error, ExecutorError},
    executor::{Executor, ExecutorEvent},
    heuristics::HeuristicsHandle,
    types::DEFAULT_CHANNEL_SIZE,
};

use tokio::sync::mpsc;

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
    time::Duration,
};

#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &str = "filter";

/// Logging target for binary messages.
const LOG_TARGET_MSG: &str = "filter::msg";

/// Events produced by [`Filter`].
#[derive(Debug)]
pub enum FilterEvent<T: NetworkBackend> {
    /// Connect to peer.
    Connect {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Peer ID.
        peer: T::PeerId,
    },
}

/// Commands sent by `Overseer` to [`Filter`].
#[derive(Debug)]
pub enum FilterCommand<T: NetworkBackend> {
    /// Register peer to [`Filter`].
    RegisterPeer {
        /// Peer ID.
        peer: T::PeerId,

        /// Packet sink.
        sink: Box<dyn PacketSink<T>>,
    },

    /// Discover peer.
    DiscoverPeer {
        /// Peer ID.
        peer: T::PeerId,
    },

    /// Unregister peer from [`Filter`].
    UnregisterPeer {
        /// Peer ID.
        peer: T::PeerId,
    },

    /// Peer opened a protocol.
    ProtocolOpened {
        /// Peer ID.
        peer: T::PeerId,

        /// Protocol.
        protocol: T::Protocol,
    },

    /// Peer closed a protocol.
    ProtocolClosed {
        /// Peer ID.
        peer: T::PeerId,

        /// Protocol.
        protocol: T::Protocol,
    },

    /// Install notification filter.
    InstallNotificationFilter {
        /// Protocol.
        protocol: T::Protocol,

        /// Filter code.
        filter: String,

        /// Optional filter context.
        context: Option<String>,
    },

    /// Install request filter.
    InstallRequestResponseFilter {
        /// Protocol.
        protocol: T::Protocol,

        /// Filter code.
        filter: String,

        /// Optional filter context.
        context: Option<String>,
    },

    /// Inject notification to filter.
    InjectNotification {
        /// Protocol.
        protocol: T::Protocol,

        /// Peer ID.
        peer: T::PeerId,

        /// Notification.
        notification: T::Message,
    },

    InjectRequest {
        /// Protocol.
        protocol: T::Protocol,

        /// Peer ID.
        peer: T::PeerId,

        /// Request.
        request: T::Request,
    },

    /// Inject response to filter.
    InjectResponse {
        /// Protocol.
        protocol: T::Protocol,

        /// Peer ID.
        peer: T::PeerId,

        /// Response.
        response: T::Response,
    },
}

/// Handle which allows allows `Overseer` to interact with filter
pub struct FilterHandle<T: NetworkBackend> {
    /// TX channel for sending commands to [`Filter`].
    tx: mpsc::Sender<FilterCommand<T>>,
}

impl<T: NetworkBackend> FilterHandle<T> {
    /// Create new [`FilterHandle`].
    pub fn new(tx: mpsc::Sender<FilterCommand<T>>) -> Self {
        Self { tx }
    }

    /// Register peer to [`Filter`].
    pub async fn register_peer(&self, peer: T::PeerId, sink: Box<dyn PacketSink<T>>) {
        self.tx
            .send(FilterCommand::RegisterPeer { peer, sink })
            .await
            .expect("channel to stay open");
    }

    /// Discover peer.
    pub async fn discover_peer(&self, peer: T::PeerId) {
        self.tx
            .send(FilterCommand::DiscoverPeer { peer })
            .await
            .expect("channel to stay open");
    }

    /// Register peer from [`Filter`].
    pub async fn unregister_peer(&self, peer: T::PeerId) {
        self.tx
            .send(FilterCommand::UnregisterPeer { peer })
            .await
            .expect("channel to stay open");
    }

    /// Protocol opened.
    pub async fn protocol_opened(&mut self, peer: T::PeerId, protocol: T::Protocol) {
        self.tx
            .send(FilterCommand::ProtocolOpened { peer, protocol })
            .await
            .expect("channel to stay open");
    }

    /// Protocol closed.
    pub async fn protocol_closed(&mut self, peer: T::PeerId, protocol: T::Protocol) {
        self.tx
            .send(FilterCommand::ProtocolClosed { peer, protocol })
            .await
            .expect("channel to stay open");
    }

    /// Install notification filter.
    pub async fn install_notification_filter(
        &self,
        protocol: T::Protocol,
        filter: String,
        context: Option<String>,
    ) {
        self.tx
            .send(FilterCommand::InstallNotificationFilter {
                protocol,
                filter,
                context,
            })
            .await
            .expect("channel to stay open");
    }

    /// Install request filter.
    pub async fn install_request_response_filter(
        &self,
        protocol: T::Protocol,
        filter: String,
        context: Option<String>,
    ) {
        self.tx
            .send(FilterCommand::InstallRequestResponseFilter {
                protocol,
                filter,
                context,
            })
            .await
            .expect("channel to stay open");
    }

    /// Inject notification to filter.
    pub async fn inject_notification(
        &self,
        protocol: T::Protocol,
        peer: T::PeerId,
        notification: T::Message,
    ) {
        self.tx
            .send(FilterCommand::InjectNotification {
                protocol,
                peer,
                notification,
            })
            .await
            .expect("channel to stay open");
    }

    /// Inject request to filter.
    pub async fn inject_request(
        &self,
        protocol: T::Protocol,
        peer: T::PeerId,
        request: T::Request,
    ) {
        self.tx
            .send(FilterCommand::InjectRequest {
                protocol,
                peer,
                request,
            })
            .await
            .expect("channel to stay open");
    }

    /// Inject response to filter.
    pub async fn inject_response(
        &self,
        protocol: T::Protocol,
        peer: T::PeerId,
        response: T::Response,
    ) {
        self.tx
            .send(FilterCommand::InjectResponse {
                protocol,
                peer,
                response,
            })
            .await
            .expect("channel to stay open");
    }
}

/// Peer information.
struct PeerInfo<T: NetworkBackend> {
    /// Packet sink.
    sink: Box<dyn PacketSink<T>>,

    /// Open protocols.
    protocols: HashSet<T::Protocol>,
}

/// Message filter.
pub struct Filter<T: NetworkBackend, E: Executor<T>> {
    /// Executor.
    executor: E,

    /// Interface ID.
    interface: T::InterfaceId,

    /// RX channel for listening to commands from `Overseer`.
    command_rx: mpsc::Receiver<FilterCommand<T>>,

    /// TX channel for sending events to `Overseer`.
    event_tx: mpsc::Sender<FilterEvent<T>>,

    /// Registered peers.
    peers: HashMap<T::PeerId, PeerInfo<T>>,

    // Pending inbound requests.
    pending_inbound: HashMap<T::PeerId, T::RequestId>,

    // Pending outbound requests.
    _pending_outbound: HashMap<T::RequestId, T::RequestId>,

    /// Heuristics handle.
    heuristics_handle: HeuristicsHandle<T>,

    /// Poll interval.
    poll_interval: Duration,
}

impl<T: NetworkBackend, E: Executor<T>> Filter<T, E> {
    pub fn new(
        interface: T::InterfaceId,
        filter: String,
        poll_interval: Duration,
        event_tx: mpsc::Sender<FilterEvent<T>>,
        heuristics_handle: HeuristicsHandle<T>,
    ) -> crate::Result<(Filter<T, E>, FilterHandle<T>)> {
        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        Ok((
            Filter::<T, E> {
                interface,
                command_rx: rx,
                event_tx,
                executor: E::new(interface, filter, None)?,
                heuristics_handle,
                poll_interval,
                peers: HashMap::new(),
                pending_inbound: HashMap::new(),
                _pending_outbound: HashMap::new(),
            },
            FilterHandle::new(tx),
        ))
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                command = self.command_rx.recv() => match command.expect("channel to stay open ") {
                    FilterCommand::RegisterPeer { peer, sink } => {
                        if let Err(error) = self.register_peer(peer, sink).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?peer,
                                ?error,
                                "failed to register peer",
                            );
                        }
                    }
                    FilterCommand::DiscoverPeer { peer } => {
                        if let Err(error) = self.discover_peer(peer).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?peer,
                                ?error,
                                "failed to discover peer",
                            );
                        }
                    }
                    FilterCommand::UnregisterPeer { peer } => {
                        if let Err(error) = self.unregister_peer(peer).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?peer,
                                ?error,
                                "failed to unregister peer",
                            );
                        }
                    }
                    FilterCommand::ProtocolOpened { peer, protocol } => {
                        if let Err(error) = self.protocol_opened(peer, protocol.clone()).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?peer,
                                ?protocol,
                                ?error,
                                "failed to register protocol opened event",
                            );
                        }
                    }
                    FilterCommand::ProtocolClosed { peer, protocol } => {
                        if let Err(error) = self.protocol_closed(peer, protocol.clone()).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?peer,
                                ?protocol,
                                ?error,
                                "failed to register protocol closed event",
                            );
                        }
                    }
                    FilterCommand::InstallNotificationFilter {
                        protocol,
                        filter,
                        context: _,
                    } => {
                        if let Err(error) = self.install_notification_filter(protocol.clone(), filter) {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?protocol,
                                ?error,
                                "failed to install notification filter",
                            );
                        }
                    }
                    FilterCommand::InstallRequestResponseFilter {
                        protocol,
                        filter,
                        context: _,
                    } => {
                        if let Err(error) = self.install_request_response_filter(protocol.clone(), filter) {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?protocol,
                                ?error,
                                "failed to install request-response filter",
                            );
                        }
                    }
                    FilterCommand::InjectNotification {
                        peer,
                        protocol,
                        notification,
                    } => {
                        if let Err(error) = self.inject_notification(protocol.clone(), peer, notification).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?protocol,
                                ?peer,
                                ?error,
                                "failed to inject notification",
                            );
                        }
                    }
                    FilterCommand::InjectRequest {
                        peer,
                        protocol,
                        request,
                    } => {
                        if let Err(error) = self.inject_request(protocol.clone(), peer, request).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?protocol,
                                ?peer,
                                ?error,
                                "failed to inject request",
                            );
                        }
                    }
                    FilterCommand::InjectResponse {
                        protocol,
                        peer,
                        response,
                    } => {
                        if let Err(error) = self.inject_response(protocol.clone(), peer, response).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?protocol,
                                ?error,
                                "failed to inject response",
                            );
                        }
                    }
                },
                _ = tokio::time::sleep(self.poll_interval) => {
                    if let Err(error) = self.poll_filter().await {
                        tracing::error!(
                            target: LOG_TARGET,
                            interface = ?self.interface,
                            ?error,
                            "failed to poll filter",
                        );
                    }
                },
            }
        }
    }

    /// Process events received from the executor.
    async fn process_events(&mut self, events: Vec<ExecutorEvent<T>>) -> crate::Result<()> {
        for event in events {
            match event {
                ExecutorEvent::Connect { peer } => self
                    .event_tx
                    .send(FilterEvent::Connect {
                        interface: self.interface,
                        peer,
                    })
                    .await
                    .expect("channel to stay open"),
                ExecutorEvent::Forward {
                    peers,
                    protocol,
                    notification,
                } => {
                    tracing::info!(
                        target: LOG_TARGET,
                        interface = ?self.interface,
                        ?protocol,
                        ?peers,
                        "forward notification",
                    );

                    for peer in peers {
                        let Some(info) = self.peers.get_mut(&peer) else {
                            tracing::debug!(target: LOG_TARGET, ?peer, "peer doesn't exist");
                            continue;
                        };

                        match info
                            .sink
                            .send_packet(Some(protocol.clone()), &notification)
                            .await
                        {
                            Ok(_) => {
                                self.heuristics_handle.register_notification_sent(
                                    self.interface,
                                    protocol.clone(),
                                    vec![peer],
                                    &notification,
                                );
                            }
                            Err(err) => {
                                tracing::warn!(
                                    target: LOG_TARGET,
                                    ?err,
                                    "failed to send notification"
                                );
                            }
                        }
                    }
                }
                ExecutorEvent::SendRequest {
                    protocol,
                    peer,
                    payload,
                } => {
                    tracing::trace!(target: LOG_TARGET, interface = ?self.interface, ?peer, "send request");
                    tracing::trace!(target: LOG_TARGET_MSG, ?payload);

                    // TODO: insert into `self.pending_outbound` maybe?

                    if let Err(error) = self
                        .peers
                        .get_mut(&peer)
                        .ok_or(Error::PeerDoesntExist)?
                        .sink
                        .send_request(protocol.clone(), payload)
                        .await
                        .map(|_| ())
                    {
                        tracing::error!(
                            target: LOG_TARGET,
                            ?error,
                            ?protocol,
                            ?peer,
                            "failed to send request"
                        );
                    }
                }
                ExecutorEvent::SendResponse { peer, payload } => {
                    match self.pending_inbound.remove(&peer) {
                        Some(request_id) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                interface = ?self.interface,
                                ?peer,
                                ?request_id,
                                "send response"
                            );
                            tracing::trace!(target: LOG_TARGET_MSG, ?payload);

                            match self
                                .peers
                                .get_mut(&peer)
                                .ok_or(Error::PeerDoesntExist)?
                                .sink
                                .send_response(request_id, payload)
                                .await
                            {
                                Ok(_) => {
                                    // TODO: register the new request
                                    // self.heuristics_handle.register_request_sent(
                                    //     self.interface,
                                    //     protocol.to_owned(),
                                    //     peer,
                                    //     &response,
                                    // );
                                }
                                Err(error) => {
                                    tracing::warn!(
                                        target: LOG_TARGET,
                                        ?request_id,
                                        ?peer,
                                        ?error,
                                        "failed to send response"
                                    );
                                }
                            }
                        }
                        None => {
                            tracing::warn!(
                                target: LOG_TARGET,
                                ?peer,
                                "tried to respond to request that doesn't exist"
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Poll the installed filter.
    async fn poll_filter(&mut self) -> crate::Result<()> {
        let events = self.executor.poll()?;

        self.process_events(events).await
    }

    /// Register peer to [`Filter`].
    async fn register_peer(
        &mut self,
        peer: T::PeerId,
        sink: Box<dyn PacketSink<T>>,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, inteface = ?self.interface, ?peer, "register peer");

        match self.peers.entry(peer) {
            Entry::Occupied(_) => Err(Error::PeerAlreadyExists),
            Entry::Vacant(entry) => {
                let events = self.executor.register_peer(peer)?;

                entry.insert(PeerInfo {
                    sink,
                    protocols: HashSet::new(),
                });
                self.process_events(events).await
            }
        }
    }

    async fn discover_peer(&mut self, peer: T::PeerId) -> crate::Result<()> {
        let events = self.executor.discover_peer(peer)?;
        self.process_events(events).await
    }

    /// Unregister peer to [`Filter`].
    async fn unregister_peer(&mut self, peer: T::PeerId) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, inteface = ?self.interface, ?peer, "unregister peer");

        let events = self
            .peers
            .remove(&peer)
            .map_or(Err(Error::PeerDoesntExist), |_| {
                self.executor.unregister_peer(peer)
            })?;

        self.process_events(events).await
    }

    /// Register protocol opened event to executor.
    async fn protocol_opened(
        &mut self,
        peer: T::PeerId,
        protocol: T::Protocol,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, inteface = ?self.interface, ?peer, ?protocol, "protocol opened");

        let Some(info) = self.peers.get_mut(&peer) else {
            return Err(Error::PeerDoesntExist);
        };

        info.protocols.insert(protocol.clone());
        let events = self.executor.protocol_opened(peer, protocol)?;
        self.process_events(events).await
    }

    /// Register protocol closed event to executor.
    async fn protocol_closed(
        &mut self,
        peer: T::PeerId,
        protocol: T::Protocol,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, inteface = ?self.interface, ?peer, ?protocol, "protocol closed");

        let Some(info) = self.peers.get_mut(&peer) else {
            return Err(Error::PeerDoesntExist);
        };

        info.protocols.insert(protocol.clone());
        let events = self.executor.protocol_closed(peer, protocol)?;
        self.process_events(events).await
    }

    /// Install notification filter.
    fn install_notification_filter(
        &mut self,
        protocol: T::Protocol,
        filter: String,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, inteface = ?self.interface, ?protocol, "install notification filter");

        self.executor.install_notification_filter(protocol, filter)
    }

    /// Install request-response filter.
    fn install_request_response_filter(
        &mut self,
        protocol: T::Protocol,
        filter: String,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            inteface = ?self.interface,
            ?protocol,
            "install request-response filter"
        );

        self.executor
            .install_request_response_filter(protocol, filter)
    }

    /// Inject notification to filter.
    async fn inject_notification(
        &mut self,
        protocol: T::Protocol,
        peer: T::PeerId,
        notification: T::Message,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, inteface = ?self.interface, ?protocol, "inject notification");

        // register the received notification to heuristics backend
        self.heuristics_handle.register_notification_received(
            self.interface,
            protocol.to_owned(),
            peer,
            &notification,
        );

        match self
            .executor
            .inject_notification(protocol.clone(), peer, notification.clone())
        {
            Ok(events) => self.process_events(events).await,
            Err(Error::ExecutorError(ExecutorError::FilterDoesntExist)) => {
                tracing::trace!(
                    target: LOG_TARGET,
                    inteface = ?self.interface,
                    ?protocol,
                    "filter does not exist, forward to all peers by default",
                );

                for (peer, info) in self.peers.iter_mut() {
                    match info
                        .sink
                        .send_packet(Some(protocol.clone()), &notification)
                        .await
                    {
                        Ok(_) => {
                            self.heuristics_handle.register_notification_sent(
                                self.interface,
                                protocol.clone(),
                                vec![*peer],
                                &notification,
                            );
                        }
                        Err(err) => {
                            tracing::warn!(target: LOG_TARGET, inteface = ?self.interface, ?err, "failed to send notification");
                        }
                    }
                }

                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Inject request to filter.
    async fn inject_request(
        &mut self,
        protocol: T::Protocol,
        peer: T::PeerId,
        request: T::Request,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, inteface = ?self.interface, ?peer, ?protocol, request_id = ?request.id(), "inject request");
        tracing::trace!(target: LOG_TARGET_MSG, interface = ?self.interface, ?request);

        // save the id of the received request so later on the response received from the executor
        // can be associated with the correct request.
        //
        // also register the received request to heuristics backend.
        self.pending_inbound.insert(peer, *request.id());
        self.heuristics_handle.register_request_received(
            self.interface,
            protocol.clone(),
            peer,
            &request,
        );

        let events = self.executor.inject_request(protocol, peer, request)?;

        self.process_events(events).await
    }

    /// Inject response to filter.
    async fn inject_response(
        &mut self,
        protocol: T::Protocol,
        peer: T::PeerId,
        response: T::Response,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, inteface = ?self.interface, ?protocol, ?peer, "inject response");
        tracing::trace!(target: LOG_TARGET_MSG, interface = ?self.interface, ?response);

        // register the received response to heuristics backend
        self.heuristics_handle.register_response_received(
            self.interface,
            protocol.clone(),
            peer,
            &response,
        );

        let events = self.executor.inject_response(protocol, peer, response)?;
        self.process_events(events).await
    }
}
