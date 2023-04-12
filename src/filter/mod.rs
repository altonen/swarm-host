//! Message filtering implementation.

use crate::{
    backend::{InterfaceType, NetworkBackend},
    ensure,
    error::{Error, FilterError},
    executor::Executor,
    types::DEFAULT_CHANNEL_SIZE,
};

use petgraph::{
    graph::{DiGraph, EdgeIndex, NodeIndex},
    visit::{Dfs, Walker},
};
use pyo3::prelude::*;
use tokio::sync::mpsc;
use tracing::Level;

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
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

// TODO: remove?
/// Filtering mode for peer/interface.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FilterType {
    /// Forward all messages to other peers of the interface.
    FullBypass,

    /// Drop all messages received to the interface.
    DropAll,
}

// TODO: documentation
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LinkType {
    // TODO: documentation
    IngressOnly,

    // TODO: documentation
    EgressOnly,

    // TODO: documentation
    Bidrectional,
}

// TODO: documentation
pub enum EdgeType {
    // TODO: documentation
    Unidirectional(EdgeIndex),

    // TODO: documentation
    Bidrectional(EdgeIndex, EdgeIndex),
}

/// Request handling reusult.
///
/// Returned either by the installed filter or by the default handler
/// based on the interface type.
pub enum RequestHandlingResult {
    /// Cause timeout for the request.
    Timeout,

    /// Reject the request.
    Reject,

    /// Forward the request to designated peer.
    // TODO: who is the request forwarded to?
    Forward,
}

/// Response handling reusult.
///
/// Returned either by the installed filter or by the default handler
/// based on the interface type.
pub enum ResponseHandlingResult {
    /// Cause timeout for the request.
    Timeout,

    /// Reject the request.
    Reject,

    /// Forward the request to destination.
    Forward,
}

/// Interface-related information.
#[derive(Debug)]
struct Interface<T: NetworkBackend> {
    /// Message filtering mode.
    filter: FilterType,

    /// Connected peers of the interface.
    peers: HashSet<T::PeerId>,

    /// Established links between interfaces.
    links: HashSet<T::InterfaceId>,

    /// Index to the link graph.
    index: NodeIndex,

    /// Notification filters.
    notification_filters: HashMap<T::Protocol, (String, String)>,
}

/// Object implementing message filtering for `swarm-host`.
pub struct MessageFilter<T: NetworkBackend> {
    /// Filtering modes installed for connected peers.
    peer_filters: HashMap<(T::InterfaceId, T::PeerId), FilterType>,

    /// Installed interfaces and their information.
    interfaces: HashMap<T::InterfaceId, Interface<T>>,

    /// Edges of the link graph.
    edges: HashMap<(T::InterfaceId, T::InterfaceId), EdgeType>,

    /// Linked interfaces.
    // TODO: store `Interface<T>` here?
    links: DiGraph<T::InterfaceId, ()>,

    /// Packet filters.
    filters: HashMap<
        T::InterfaceId,
        Box<
            dyn Fn(T::InterfaceId, T::PeerId, T::InterfaceId, T::PeerId, &T::Message) -> bool
                + Send,
        >,
    >,
}

impl<T: NetworkBackend + Debug> MessageFilter<T> {
    /// Create new [`MessageFilter`].
    pub fn new() -> Self {
        Self {
            peer_filters: HashMap::new(),
            interfaces: HashMap::new(),
            links: DiGraph::new(),
            edges: HashMap::new(),
            filters: HashMap::new(),
        }
    }

    /// Register interface to [`MessageFilter`].
    pub fn register_interface(
        &mut self,
        interface: T::InterfaceId,
        filter: FilterType,
    ) -> crate::Result<()> {
        tracing::info!(
            target: LOG_TARGET,
            ?interface,
            ?filter,
            "register interface",
        );

        match self.interfaces.entry(interface) {
            Entry::Occupied(_) => return Err(Error::InterfaceAlreadyExists),
            Entry::Vacant(entry) => {
                entry.insert(Interface {
                    filter,
                    index: self.links.add_node(interface),
                    peers: Default::default(),
                    links: Default::default(),
                    notification_filters: Default::default(),
                });

                Ok(())
            }
        }
    }

    /// Link interfaces together `first` to `second`.
    ///
    /// Parameter `link` defines how packet flow between the interfaces work:
    ///   - `LinkType::IngressOnly`: `first` <- `second`
    ///   - `LinkType::EgressOnly`: `first` -> `second`
    ///   - `LinkType::Bidrectional`: `first` <-> `second`
    pub fn link_interface(
        &mut self,
        first: T::InterfaceId,
        second: T::InterfaceId,
        link: LinkType,
    ) -> crate::Result<()> {
        let first_idx = self
            .interfaces
            .get(&first)
            .ok_or(Error::InterfaceDoesntExist)?
            .index;
        let second_idx = self
            .interfaces
            .get(&second)
            .ok_or(Error::InterfaceDoesntExist)?
            .index;

        tracing::info!(
            target: LOG_TARGET,
            interface = ?first,
            interface = ?second,
            ?link,
            "link interfaces",
        );

        match link {
            LinkType::IngressOnly => {
                self.edges.insert(
                    (first, second),
                    EdgeType::Unidirectional(self.links.add_edge(second_idx, first_idx, ())),
                );
            }
            LinkType::EgressOnly => {
                self.edges.insert(
                    (first, second),
                    EdgeType::Unidirectional(self.links.add_edge(first_idx, second_idx, ())),
                );
            }
            LinkType::Bidrectional => {
                self.edges.insert(
                    (first, second),
                    EdgeType::Bidrectional(
                        self.links.add_edge(first_idx, second_idx, ()),
                        self.links.add_edge(second_idx, first_idx, ()),
                    ),
                );
            }
        }

        Ok(())
    }

    /// Unlink interfaces.
    ///
    /// Remove previously established link between the interfaces. Peers of interface 1
    /// no longer receives packets from interface 2.
    pub fn unlink_interface(
        &mut self,
        first: T::InterfaceId,
        second: T::InterfaceId,
    ) -> crate::Result<()> {
        ensure!(
            self.interfaces.contains_key(&first) && self.interfaces.contains_key(&second),
            Error::InterfaceDoesntExist,
        );

        tracing::info!(
            target: LOG_TARGET,
            interface = ?first,
            interface = ?second,
            "link interfaces",
        );

        match self.edges.remove(&(first, second)) {
            Some(EdgeType::Unidirectional(edge)) => {
                self.links.remove_edge(edge);
            }
            Some(EdgeType::Bidrectional(edge1, edge2)) => {
                self.links.remove_edge(edge1);
                self.links.remove_edge(edge2);
            }
            None => return Err(Error::LinkDoesntExist),
        }

        Ok(())
    }

    /// Register peer to [`MessageFilter`].
    pub fn register_peer(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        filter: FilterType,
    ) -> crate::Result<()> {
        ensure!(
            self.interfaces.contains_key(&interface),
            Error::InterfaceDoesntExist,
        );
        ensure!(
            !self.peer_filters.contains_key(&(interface, peer)),
            Error::PeerAlreadyExists,
        );

        tracing::debug!(
            target: LOG_TARGET,
            interface_id = ?interface,
            peer_id = ?peer,
            filter = ?filter,
            "register peer",
        );

        self.peer_filters.insert((interface, peer), filter);
        self.interfaces
            .get_mut(&interface)
            .expect("entry to exist")
            .peers
            .insert(peer);

        Ok(())
    }

    /// Add custom filter for notifications.
    pub fn install_notification_filter(
        &mut self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        context: String,
        filter_code: String,
    ) -> crate::Result<()> {
        let iface = self
            .interfaces
            .get_mut(&interface)
            .ok_or(Error::InterfaceDoesntExist)?;

        tracing::debug!(
            target: LOG_TARGET,
            ?interface,
            ?protocol,
            "install notification filter"
        );

        tracing::info!(target: LOG_TARGET, filter_code);

        // TODO: abstract this behind some interface
        Python::with_gil(|py| -> Result<(), Error> {
            PyModule::from_code(py, &filter_code, "", "")
                .map_err(|error| {
                    tracing::error!(
                        target:LOG_TARGET,
                        interface_id = ?interface,
                        ?error,
                        ?filter_code,
                        "`PyO3` failed to initialize filter code",
                    );
                    Error::FilterError(FilterError::InvalidFilter(filter_code.clone()))
                })?
                .getattr("filter_notification")
                .map_err(|error| {
                    tracing::error!(
                        target: LOG_TARGET,
                        interface_id = ?interface,
                        ?error,
                        ?filter_code,
                        "didn't find `filter_notification()` from supplied code",
                    );
                    Error::FilterError(FilterError::InvalidFilter(filter_code.clone()))
                })
                .map(|_| {
                    iface
                        .notification_filters
                        .insert(protocol, (context, filter_code));
                    ()
                })
        })
    }

    /// Add custom filter for requests.
    pub fn install_request_filter(&mut self, interface: T::InterfaceId) -> crate::Result<()> {
        todo!();
    }

    /// Add custom filter for respones.
    pub fn install_response_filter(&mut self, interface: T::InterfaceId) -> crate::Result<()> {
        todo!();
    }

    /// Inject notification into [`MessageFilter`].
    ///
    /// The notification is processed based on the source peer and interface IDs and notification
    /// type using any user-installed filters to further alter the notification processing.
    ///
    /// After the processing is done, a list of `(interface ID, peer ID)` pairs are returned
    /// to caller who can then forward the notification to correct peers.
    pub fn inject_notification(
        &mut self,
        src_interface: T::InterfaceId,
        src_peer: T::PeerId,
        protocol: &T::Protocol,
        message: &T::Message,
    ) -> crate::Result<(impl Iterator<Item = (T::InterfaceId, T::PeerId)>)> {
        let iface_idx = self
            .interfaces
            .get(&src_interface)
            .ok_or(Error::InterfaceDoesntExist)?
            .index;

        tracing::span!(target: LOG_TARGET, Level::DEBUG, "inject notification").enter();
        tracing::event!(
            target: LOG_TARGET,
            Level::DEBUG,
            peer = ?src_peer,
            interface = ?src_interface,
            ?protocol,
            "inject notification",
        );
        tracing::event!(target: LOG_TARGET_MSG, Level::TRACE, ?message,);

        let pairs = Dfs::new(&self.links, iface_idx)
            .iter(&self.links)
            .map(|nx| {
                let dst_interface = self.links.node_weight(nx).expect("entry to exist");
                let dst_iface = self
                    .interfaces
                    .get(dst_interface)
                    .expect("interface to exist");

                match dst_iface.filter {
                    FilterType::DropAll => None,
                    FilterType::FullBypass => Some(
                        dst_iface
                            .peers
                            .iter()
                            .filter_map(|&dst_peer| {
                                // don't forward message to source
                                if dst_interface == &src_interface && dst_peer == src_peer {
                                    return None;
                                }

                                match dst_iface.notification_filters.get(&protocol) {
                                    Some((context, filter)) => {
                                        let result = Python::with_gil(|py| {
                                            tracing::event!(
                                                target: LOG_TARGET,
                                                Level::DEBUG,
                                                "execute custom notification filter",
                                            );

                                            let fun = PyModule::from_code(py, &filter, "", "")
                                                .expect("code to be loaded successfully")
                                                .getattr("filter_notification")
                                                .expect("`filter_notifications()` to exist");

                                            // TODO: remove clones
                                            // TODO: absolutely hideous
                                            // TODO: add ability to pass context as mutable reference
                                            let result = fun
                                                .call1((
                                                    context.clone().into_py(py),
                                                    src_interface,
                                                    src_peer,
                                                    *dst_interface,
                                                    dst_peer,
                                                    message.clone(),
                                                ))
                                                .unwrap()
                                                .extract::<bool>()
                                                .unwrap();

                                            tracing::event!(
                                                target: LOG_TARGET,
                                                Level::DEBUG,
                                                ?result,
                                                "filter execution done",
                                            );

                                            result
                                        });

                                        if result {
                                            tracing::event!(
                                                target: LOG_TARGET,
                                                Level::DEBUG,
                                                interface_id = ?dst_interface,
                                                peer_id = ?dst_peer,
                                                ?protocol,
                                                "forward message to peer",
                                            );
                                            Some((*dst_interface, dst_peer))
                                        } else {
                                            return None;
                                        }
                                    }
                                    None => Some((*dst_interface, dst_peer)),
                                }
                            })
                            .collect::<Vec<_>>(),
                    ),
                }
            })
            .flatten()
            .flatten()
            .collect::<Vec<_>>();

        Ok(pairs.into_iter())
    }

    //// Inject request into the [`MessageFilter`].
    ///
    /// Requests are handled by default by sending them to the bound interface.
    pub fn inject_request(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        protocol: &T::Protocol,
        request: &T::Request,
    ) -> RequestHandlingResult {
        tracing::warn!(target: LOG_TARGET, ?peer, ?interface, "inject request");
        tracing::trace!(target: LOG_TARGET_MSG, ?request);

        RequestHandlingResult::Forward
    }

    //// Inject response into the [`MessageFilter`].
    ///
    /// Responses are handled by default by sending them to the source.
    pub fn inject_response(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        protocol: &T::Protocol,
        response: &T::Response,
    ) -> ResponseHandlingResult {
        tracing::warn!(target: LOG_TARGET, ?peer, ?interface, "inject response");
        tracing::trace!(target: LOG_TARGET_MSG, ?response);

        ResponseHandlingResult::Forward
    }
}

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
    pub async fn register_peer(&self, peer: T::PeerId) {
        self.tx
            .send(FilterCommand::RegisterPeer { peer })
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
    peers: HashMap<T::PeerId, ()>,
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
            },
            FilterHandle::new(tx),
        )
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                command = self.command_rx.recv() => match command.expect("channel to stay open ") {
                    FilterCommand::RegisterPeer { peer } => {
                        if let Err(error) = self.register_peer(peer) {
                            tracing::error!(
                                target: LOG_TARGET,
                                ?peer,
                                ?error,
                                "failed to register peer",
                            );
                        }
                    }
                    FilterCommand::UnregisterPeer { peer } => {
                        if let Err(error) = self.unregister_peer(&peer) {
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
                        if let Err(error) = self.inject_notification(&protocol, peer, notification) {
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
                }
            }
        }
    }

    /// Register peer to [`Filter`].
    fn register_peer(&mut self, peer: T::PeerId) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?peer, "register peer");

        match self.peers.entry(peer) {
            Entry::Vacant(entry) => {
                entry.insert(());
                Ok(())
            }
            Entry::Occupied(_) => Err(Error::PeerAlreadyExists),
        }
    }

    /// Unregister peer to [`Filter`].
    fn unregister_peer(&mut self, peer: &T::PeerId) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?peer, "unregister peer");

        self.peers
            .remove(&peer)
            .map_or(Err(Error::PeerDoesntExist), |_| Ok(()))
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
            .ok_or(Error::ExecutorDoesntExist)?
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
    fn inject_notification(
        &self,
        protocol: &T::Protocol,
        peer: T::PeerId,
        notification: T::Message,
    ) -> crate::Result<()> {
        todo!();
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
