//! Message filtering implementation.

use crate::{
    backend::{InterfaceType, NetworkBackend, PacketSink},
    ensure,
    error::{Error, ExecutorError, FilterError},
    executor::{Executor, NotificationHandlingResult},
    types::DEFAULT_CHANNEL_SIZE,
};

use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use petgraph::{
    graph::{DiGraph, EdgeIndex, NodeIndex},
    visit::{Dfs, Walker},
};
use pyo3::prelude::*;
use tokio::{sync::mpsc, time};
use tracing::Level;

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
    time::Duration,
};

#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &'static str = "filter";

/// Logging target for binary messages.
const LOG_TARGET_MSG: &'static str = "filter::msg";

// TODO: move interface linking to overseer
// TODO: move all `pyo3`-related code behind an interface
// TODO: create separate filter task for each new interface
// TODO: message -> notification
// TODO: implement `freeze()` which hard-codes the paths -> no expensive calculations on each message
// TODO: start using `mockall`
// TODO: documentation
// TODO: fuzzing
// TODO: benches

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
    InstallRequestFilter {
        /// Protocol.
        protocol: T::Protocol,

        /// Filter code.
        filter: String,

        /// Optional filter context.
        context: Option<String>,
    },

    /// Install response filter.
    InstallResponseFilter {
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

        /// Request ID.
        request_id: T::RequestId,

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
    pub async fn install_request_filter(
        &self,
        protocol: T::Protocol,
        filter: String,
        context: Option<String>,
    ) {
        self.tx
            .send(FilterCommand::InstallRequestFilter {
                protocol,
                filter,
                context,
            })
            .await
            .expect("channel to stay open");
    }

    /// Install response filter.
    pub async fn install_response_filter(
        &self,
        protocol: T::Protocol,
        filter: String,
        context: Option<String>,
    ) {
        self.tx
            .send(FilterCommand::InstallResponseFilter {
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
        request_id: T::RequestId,
        response: T::Response,
    ) {
        self.tx
            .send(FilterCommand::InjectResponse {
                protocol,
                request_id,
                response,
            })
            .await
            .expect("channel to stay open");
    }
}

pub struct Filter<T: NetworkBackend, E: Executor<T>> {
    /// Executor.
    executor: Option<E>,

    /// Interface ID.
    interface: T::InterfaceId,

    /// RX channel for listening to commands from `Overseer`.
    command_rx: mpsc::Receiver<FilterCommand<T>>,

    /// TX channel for sending events to `Overseer`.
    event_tx: mpsc::Sender<FilterEvent>,

    /// Registered peers.
    peers: HashMap<T::PeerId, Box<dyn PacketSink<T>>>,

    // Delayed notifications.
    delayed_notifications: FuturesUnordered<BoxFuture<'static, T::Message>>,
}

impl<T: NetworkBackend, E: Executor<T>> Filter<T, E> {
    pub fn new(
        interface: T::InterfaceId,
        event_tx: mpsc::Sender<FilterEvent>,
    ) -> (Filter<T, E>, FilterHandle<T>) {
        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        (
            Filter::<T, E> {
                interface,
                command_rx: rx,
                event_tx,
                peers: HashMap::new(),
                executor: None,
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
                        todo!();
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
                        todo!();
                    }
                    FilterCommand::InstallRequestFilter {
                        protocol,
                        filter,
                        context,
                    } => {
                        if let Err(error) = self.install_request_filter(&protocol, filter, context) {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?protocol,
                                ?error,
                                "failed to install request filter",
                            );
                        }
                        todo!();
                    }
                    FilterCommand::InstallResponseFilter {
                        protocol,
                        filter,
                        context,
                    } => {
                        if let Err(error) = self.install_response_filter(&protocol, filter, context) {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?protocol,
                                "failed to install response filter",
                            );
                        }
                        todo!();
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
                        todo!();
                    }
                    FilterCommand::InjectRequest {
                        peer,
                        protocol,
                        request,
                    } => {
                        if let Err(error) = self.inject_request(&protocol, peer, request) {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?protocol,
                                ?peer,
                                ?error,
                                "failed to inject request",
                            );
                        }
                        todo!();
                    }
                    FilterCommand::InjectResponse {
                        protocol,
                        request_id,
                        response,
                    } => {
                        if let Err(error) = self.inject_response(&protocol, request_id, response) {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?protocol,
                                ?error,
                                "failed to inject response",
                            );
                        }
                    }
                },
                notification = self.delayed_notifications.select_next_some(), if !self.delayed_notifications.is_empty() => {
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

    /// Install request filter.
    fn install_request_filter(
        &self,
        protocol: &T::Protocol,
        filter: String,
        context: Option<String>,
    ) -> crate::Result<()> {
        todo!();
    }

    /// Install response filter.
    fn install_response_filter(
        &self,
        protocol: &T::Protocol,
        filter: String,
        context: Option<String>,
    ) -> crate::Result<()> {
        todo!();
    }

    /// Inject notification to filter.
    // TODO: should this take source where source `Enum { Peer(PeerId), Interface(InterfaceId, PeerId) }`
    async fn inject_notification(
        &mut self,
        protocol: &T::Protocol,
        peer: T::PeerId,
        notification: T::Message,
    ) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?protocol, "inject notification");

        match self
            .executor
            .as_mut()
            .ok_or(Error::ExecutorError(ExecutorError::ExecutorDoesntExist))?
            .inject_notification(protocol, peer, notification.clone())?
        {
            NotificationHandlingResult::Drop => {
                tracing::trace!(target: LOG_TARGET, "drop notification");
                Ok(())
            }
            NotificationHandlingResult::Delay { delay } => {
                tracing::trace!(target: LOG_TARGET, "delay forwarding the notification");

                // push the notification to list of delayed notifications
                // when the timer expires, it is handled like any other forwarded notification
                self.delayed_notifications.push(Box::pin(async move {
                    time::sleep(Duration::from_secs(delay as u64)).await;
                    notification
                }));

                Ok(())
            }
            NotificationHandlingResult::Forward => {
                tracing::trace!(
                    target: LOG_TARGET,
                    "forward notification to connected peers",
                );

                for (peer, sink) in self.peers.iter_mut() {
                    if let Err(err) = sink
                        .send_packet(Some(protocol.clone()), &notification)
                        .await
                    {
                        tracing::warn!(target: LOG_TARGET, ?err, "failed to send notification");
                    }
                }

                Ok(())
            }
        }
    }

    /// Inject request to filter.
    fn inject_request(
        &self,
        protocol: &T::Protocol,
        peer: T::PeerId,
        request: T::Request,
    ) -> crate::Result<()> {
        tracing::span!(target: LOG_TARGET, Level::TRACE, "inject_request()").entered();
        tracing::event!(
            target: LOG_TARGET,
            Level::TRACE,
            ?protocol,
            "inject request"
        );
        tracing::event!(target: LOG_TARGET_MSG, Level::TRACE, ?request);

        Ok(())
    }

    /// Inject response to filter.
    fn inject_response(
        &self,
        protocol: &T::Protocol,
        request_id: T::RequestId,
        response: T::Response,
    ) -> crate::Result<()> {
        tracing::span!(target: LOG_TARGET, Level::TRACE, "inject_response()").entered();
        tracing::event!(
            target: LOG_TARGET,
            Level::TRACE,
            ?protocol,
            ?request_id,
            "inject response",
        );
        tracing::event!(target: LOG_TARGET_MSG, Level::TRACE, ?response);

        Ok(())
    }
}
