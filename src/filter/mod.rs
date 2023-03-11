//! Message filtering implementation.

use crate::{
    backend::{InterfaceType, NetworkBackend},
    ensure,
    error::Error,
};

use petgraph::{
    graph::{DiGraph, EdgeIndex, NodeIndex},
    visit::{Dfs, Walker},
};
use pyo3::prelude::*;
use tracing::Level;

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
};

mod executor;
#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &'static str = "filter";

/// Logging target for binary messages.
const LOG_TARGET_MSG: &'static str = "filter::msg";

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
    notification_filters: HashMap<T::Protocol, String>,
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
            interface_id = ?interface,
            filter = ?filter,
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
            link = ?link,
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
        filter_code: String,
    ) -> crate::Result<()> {
        let iface = self
            .interfaces
            .get_mut(&interface)
            .ok_or(Error::InterfaceDoesntExist)?;

        tracing::debug!(
            target: LOG_TARGET,
            interface_id = ?interface,
            ?protocol,
            "install notification filter"
        );

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
                    Error::InvalidFilter(filter_code.clone())
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
                    Error::InvalidFilter(filter_code.clone())
                })
                .map(|_| {
                    iface.notification_filters.insert(protocol, filter_code);
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

        tracing::debug!(
            target: LOG_TARGET,
            peer_id = ?src_peer,
            interface_id = ?src_interface,
            ?protocol,
            "inject notification",
        );
        tracing::trace!(
            target: LOG_TARGET_MSG,
            message = ?message,
        );

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
                                    Some(filter) => {
                                        Python::with_gil(|py| {
                                            let fun = PyModule::from_code(py, &filter, "", "")
                                                .expect("code to be loaded successfully")
                                                .getattr("filter_notification")
                                                .expect("`filter_notifications()` to exist");

                                            // TODO: remove clones
                                            let result = fun
                                                .call1((
                                                    src_interface,
                                                    src_peer,
                                                    *dst_interface,
                                                    dst_peer,
                                                    protocol.clone(),
                                                    message.clone(),
                                                ))
                                                .unwrap()
                                                .extract::<bool>()
                                                .unwrap();

                                            result
                                        })
                                        .then_some((*dst_interface, dst_peer))
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
        tracing::debug!(
            target: LOG_TARGET,
            peer_id = ?peer,
            interface_id = ?interface,
            "inject request",
        );
        tracing::trace!(
            target: LOG_TARGET_MSG,
            request = ?request,
        );

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
        tracing::debug!(
            target: LOG_TARGET,
            peer_id = ?peer,
            interface_id = ?interface,
            "inject response",
        );
        tracing::trace!(
            target: LOG_TARGET_MSG,
            response = ?response,
        );

        ResponseHandlingResult::Forward
    }
}
