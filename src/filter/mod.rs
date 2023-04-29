use crate::{
    backend::{Idable, NetworkBackend, PacketSink},
    error::{Error, ExecutorError},
    executor::{
        Executor, NotificationHandlingResult, RequestHandlingResult, ResponseHandlingResult,
    },
    heuristics::HeuristicsHandle,
    types::DEFAULT_CHANNEL_SIZE,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use tokio::{sync::mpsc, time};
use tracing::Level;

use std::{
    collections::{hash_map::Entry, HashMap},
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
pub enum FilterEvent {}

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

    /// Unregister peer from [`Filter`].
    UnregisterPeer {
        /// Peer ID.
        peer: T::PeerId,
    },

    /// Install filter.
    InitializeFilter {
        /// Filter code.
        filter: String,

        /// Optional filter context.
        context: Option<String>,
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

    /// Register peer from [`Filter`].
    pub async fn unregister_peer(&self, peer: T::PeerId) {
        self.tx
            .send(FilterCommand::UnregisterPeer { peer })
            .await
            .expect("channel to stay open");
    }

    /// Initialize filter context.
    ///
    /// Pass in the initialization code and any additional context, if needed.
    pub async fn initialize_filter(&self, filter: String, context: Option<String>) {
        self.tx
            .send(FilterCommand::InitializeFilter { filter, context })
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

/// Message filter.
pub struct Filter<T: NetworkBackend, E: Executor<T>> {
    /// Executor.
    executor: Option<E>,

    /// Interface ID.
    interface: T::InterfaceId,

    /// RX channel for listening to commands from `Overseer`.
    command_rx: mpsc::Receiver<FilterCommand<T>>,

    /// TX channel for sending events to `Overseer`.
    _event_tx: mpsc::Sender<FilterEvent>,

    /// Registered peers.
    peers: HashMap<T::PeerId, Box<dyn PacketSink<T>>>,

    // Pending inbound requests.
    pending_inbound: HashMap<T::PeerId, T::RequestId>,

    // Pending outbound requests.
    _pending_outbound: HashMap<T::RequestId, T::RequestId>,

    /// Heuristics handle.
    heuristics_handle: HeuristicsHandle<T>,

    // Delayed notifications.
    delayed_notifications: FuturesUnordered<BoxFuture<'static, T::Message>>,
}

impl<T: NetworkBackend, E: Executor<T>> Filter<T, E> {
    pub fn new(
        interface: T::InterfaceId,
        _event_tx: mpsc::Sender<FilterEvent>,
        heuristics_handle: HeuristicsHandle<T>,
    ) -> (Filter<T, E>, FilterHandle<T>) {
        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        (
            Filter::<T, E> {
                interface,
                command_rx: rx,
                _event_tx,
                executor: None,
                heuristics_handle,
                peers: HashMap::new(),
                pending_inbound: HashMap::new(),
                _pending_outbound: HashMap::new(),
                delayed_notifications: FuturesUnordered::new(),
            },
            FilterHandle::new(tx),
        )
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                command = self.command_rx.recv() => match command.expect("channel to stay open ") {
                    FilterCommand::RegisterPeer { peer, sink } => {
                        if let Err(error) = self.register_peer(peer, sink) {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?peer,
                                ?error,
                                "failed to register peer",
                            );
                        }
                    }
                    FilterCommand::UnregisterPeer { peer } => {
                        if let Err(error) = self.unregister_peer(peer) {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?peer,
                                ?error,
                                "failed to unregister peer",
                            );
                        }
                    }
                    FilterCommand::InitializeFilter {
                        filter,
                        context,
                    } => {
                        if let Err(error) = self.initialize_filter(self.interface, filter, context) {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?error,
                                "failed to install filter",
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
                        if let Err(error) = self.inject_notification(&protocol, peer, notification).await {
                            tracing::error!(
                                target: LOG_TARGET,
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
                        if let Err(error) = self.inject_request(&protocol, peer, request).await {
                            tracing::error!(
                                target: LOG_TARGET,
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
                        if let Err(error) = self.inject_response(&protocol, peer, response).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?protocol,
                                ?error,
                                "failed to inject response",
                            );
                        }
                    }
                },
                _notification = self.delayed_notifications.select_next_some(), if !self.delayed_notifications.is_empty() => {
                    todo!();
                }
            }
        }
    }

    /// Register peer to [`Filter`].
    fn register_peer(
        &mut self,
        peer: T::PeerId,
        sink: Box<dyn PacketSink<T>>,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?peer, "register peer");

        match self.peers.entry(peer) {
            Entry::Vacant(entry) => {
                self.executor
                    .as_mut()
                    .ok_or(Error::ExecutorError(ExecutorError::ExecutorDoesntExist))?
                    .register_peer(peer)?;
                entry.insert(sink);
                Ok(())
            }
            Entry::Occupied(_) => Err(Error::PeerAlreadyExists),
        }
    }

    /// Unregister peer to [`Filter`].
    fn unregister_peer(&mut self, peer: T::PeerId) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?peer, "unregister peer");

        self.peers
            .remove(&peer)
            .map_or(Err(Error::PeerDoesntExist), |_| {
                self.executor
                    .as_mut()
                    .ok_or(Error::ExecutorError(ExecutorError::ExecutorDoesntExist))?
                    .unregister_peer(peer)
            })
    }

    /// Install filter context.
    fn initialize_filter(
        &mut self,
        interface: T::InterfaceId,
        filter: String,
        context: Option<String>,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?interface, "initialize new filter");

        self.executor = Some(E::new(interface, filter, context)?);
        Ok(())
    }

    /// Install notification filter.
    fn install_notification_filter(
        &mut self,
        protocol: T::Protocol,
        filter: String,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?protocol, "install notification filter");

        self.executor
            .as_mut()
            .ok_or(Error::ExecutorError(ExecutorError::ExecutorDoesntExist))?
            .install_notification_filter(protocol, filter)
    }

    /// Install request-response filter.
    fn install_request_response_filter(
        &mut self,
        protocol: T::Protocol,
        filter: String,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            ?protocol,
            "install request-response filter"
        );

        self.executor
            .as_mut()
            .ok_or(Error::ExecutorError(ExecutorError::ExecutorDoesntExist))?
            .install_request_response_filter(protocol, filter)
    }

    /// Inject notification to filter.
    async fn inject_notification(
        &mut self,
        protocol: &T::Protocol,
        peer: T::PeerId,
        notification: T::Message,
    ) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?protocol, "inject notification");

        // register the received notification to heuristics backend
        self.heuristics_handle.register_notification_received(
            self.interface,
            protocol.to_owned(),
            peer,
            &notification,
        );

        match self
            .executor
            .as_mut()
            .ok_or(Error::ExecutorError(ExecutorError::ExecutorDoesntExist))?
            .inject_notification(protocol, peer, notification.clone())
        {
            Ok(NotificationHandlingResult::Drop) => {
                tracing::trace!(target: LOG_TARGET, "drop notification");
                Ok(())
            }
            Ok(NotificationHandlingResult::Delay { delay }) => {
                tracing::trace!(target: LOG_TARGET, "delay forwarding the notification");

                // push the notification to list of delayed notifications
                // when the timer expires, it is handled like any other forwarded notification
                self.delayed_notifications.push(Box::pin(async move {
                    time::sleep(Duration::from_secs(delay as u64)).await;
                    notification
                }));

                Ok(())
            }
            Ok(NotificationHandlingResult::Forward) => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?protocol,
                    ?peer,
                    "forward notification to connected peers",
                );

                for (peer, sink) in self.peers.iter_mut() {
                    match sink
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
                            tracing::warn!(target: LOG_TARGET, ?err, "failed to send notification");
                        }
                    }
                }

                Ok(())
            }
            Err(Error::ExecutorError(ExecutorError::FilterDoesntExist)) => {
                tracing::trace!(
                    target: LOG_TARGET,
                    ?protocol,
                    "filter does not exist, forward to all peers by default",
                );

                for (peer, sink) in self.peers.iter_mut() {
                    match sink
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
                            tracing::warn!(target: LOG_TARGET, ?err, "failed to send notification");
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
        protocol: &T::Protocol,
        peer: T::PeerId,
        request: T::Request,
    ) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, ?protocol, request_id = ?request.id(), "inject request");
        tracing::trace!(target: LOG_TARGET_MSG, ?request);

        // save the id of the received request so later on the response received from the executor
        // can be associated with the correct request.
        //
        // also register the received request to heuristics backend.
        self.pending_inbound.insert(peer, *request.id());
        self.heuristics_handle.register_request_received(
            self.interface,
            protocol.to_owned(),
            peer,
            &request,
        );

        match self
            .executor
            .as_mut()
            .ok_or(Error::ExecutorError(ExecutorError::ExecutorDoesntExist))?
            .inject_request(protocol, peer, request)?
        {
            RequestHandlingResult::DoNothing => {
                tracing::trace!(target: LOG_TARGET, ?peer, "ignore request");
                Ok(())
            }
            RequestHandlingResult::Request { peer, payload } => {
                tracing::trace!(target: LOG_TARGET, ?peer, "send request");
                tracing::trace!(target: LOG_TARGET_MSG, ?payload);

                self.peers
                    .get_mut(&peer)
                    .ok_or(Error::PeerDoesntExist)?
                    .send_request(protocol.clone(), payload)
                    .await
                    .map(|_| ())
            }
            RequestHandlingResult::Response { responses } => {
                tracing::trace!(target: LOG_TARGET, number_of_responses = ?responses.len(), "send responses");

                for (peer, payload) in responses {
                    match self.pending_inbound.remove(&peer) {
                        Some(request_id) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                ?request_id,
                                "send response"
                            );

                            match self
                                .peers
                                .get_mut(&peer)
                                .ok_or(Error::PeerDoesntExist)?
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

                Ok(())
            }
        }
    }

    /// Inject response to filter.
    async fn inject_response(
        &mut self,
        protocol: &T::Protocol,
        peer: T::PeerId,
        response: T::Response,
    ) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?protocol, ?peer, "inject response");
        tracing::event!(target: LOG_TARGET_MSG, Level::TRACE, ?response);

        // register the received response to heuristics backend
        self.heuristics_handle.register_response_received(
            self.interface,
            protocol.to_owned(),
            peer,
            &response,
        );

        match self
            .executor
            .as_mut()
            .ok_or(Error::ExecutorError(ExecutorError::ExecutorDoesntExist))?
            .inject_response(protocol, peer, response)?
        {
            ResponseHandlingResult::DoNothing => Ok(()),
            ResponseHandlingResult::Response { responses, request } => {
                tracing::trace!(target: LOG_TARGET, number_of_response = ?responses.len(), "send responses");

                for (peer, payload) in responses {
                    match self.pending_inbound.remove(&peer) {
                        Some(request_id) => {
                            tracing::trace!(
                                target: LOG_TARGET,
                                ?peer,
                                ?request_id,
                                "send response"
                            );

                            match self
                                .peers
                                .get_mut(&peer)
                                .ok_or(Error::PeerDoesntExist)?
                                .send_response(request_id, payload)
                                .await
                            {
                                Ok(_) => {
                                    // TODO: register the new response
                                    // self.heuristics_handle.register_response_sent(
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

                Ok(())
            }
        }
    }
}
