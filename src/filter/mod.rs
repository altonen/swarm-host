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
use tracing::Level;

use std::collections::{hash_map::Entry, HashMap, HashSet};

#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &'static str = "filter";

/// Logging target for binary messages.
const LOG_TARGET_MSG: &'static str = "filter::msg";

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
struct Interface<T: NetworkBackend> {
    /// Message filtering mode.
    filter: FilterType,

    /// Connected peers of the interface.
    peers: HashSet<T::PeerId>,

    /// Established links between interfaces.
    links: HashSet<T::InterfaceId>,

    /// Index to the link graph.
    index: NodeIndex,
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

impl<T: NetworkBackend> MessageFilter<T> {
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
        filter: Box<
            dyn Fn(T::InterfaceId, T::PeerId, T::InterfaceId, T::PeerId, &T::Message) -> bool
                + Send,
        >,
    ) -> crate::Result<()> {
        match self.filters.entry(interface) {
            Entry::Occupied(_) => Err(Error::FilterAlreadyExists),
            Entry::Vacant(entry) => {
                entry.insert(filter);
                Ok(())
            }
        }
    }

    /// Add custom filter for requests.
    pub fn install_request_filter(&mut self, interface: T::InterfaceId) -> crate::Result<()> {
        todo!();
    }

    /// Add custom filter for respones.
    pub fn install_response_filter(&mut self, interface: T::InterfaceId) -> crate::Result<()> {
        todo!();
    }

    /// Inject message into [`MessageFilter`].
    ///
    /// The message is processed based on the source peer and interface IDs and message type
    /// using any user-installed filters to further alter the message processing.
    ///
    /// After the processing is done, a list of `(interface ID, peer ID)` pairs are returned
    /// to caller who can then forward the message to correct peers.
    pub fn inject_message(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        message: &T::Message,
    ) -> crate::Result<(impl Iterator<Item = (T::InterfaceId, T::PeerId)>)> {
        let iface_idx = self
            .interfaces
            .get(&interface)
            .ok_or(Error::InterfaceDoesntExist)?
            .index;

        tracing::debug!(
            target: LOG_TARGET,
            peer_id = ?peer,
            interface_id = ?interface,
            "inject message",
        );
        tracing::trace!(
            target: LOG_TARGET_MSG,
            message = ?message,
        );

        let pairs = Dfs::new(&self.links, iface_idx)
            .iter(&self.links)
            .map(|nx| {
                let iface_id = self.links.node_weight(nx).expect("entry to exist");
                let iface = self.interfaces.get(iface_id).expect("interface to exist");

                match iface.filter {
                    FilterType::DropAll => None,
                    FilterType::FullBypass => Some(
                        iface
                            .peers
                            .iter()
                            .filter_map(|&iface_peer| {
                                // don't forward message to source
                                if iface_id == &interface && iface_peer == peer {
                                    return None;
                                }

                                self.filters.get(iface_id).map_or(
                                    Some((*iface_id, iface_peer)),
                                    |filter| {
                                        filter(interface, peer, *iface_id, iface_peer, &message)
                                            .then_some((*iface_id, iface_peer))
                                    },
                                )
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
