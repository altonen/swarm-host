//! Message filtering implementation.

use crate::{
    backend::{InterfaceType, NetworkBackend},
    ensure,
    error::Error,
};

use tracing::Level;

use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

const LOG_TARGET: &'static str = "filter";

// TODO: hierarchy for filter rules
// TODO: start using `mockall`
// TODO: separate filter types for peers and interfaces
// TODO: more complex filters?
// TODO: filter should have apply method?
// TODO: think about how to store all peer/iface data sensibly
// TODO: link interfaces together
// TODO: separate struct for Interface
// TODO: differentiate between ingress and egress
// TODO: fuzzing
// TODO: benches

// TODO: something like this
/// ```rust
/// pub trait InterfaceFilter {
///     fn apply() -> Destinations;
/// }
/// ```

// TODO: filtertype -> forwardonly
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FilterType {
    /// Forward all messages to other peers of the interface.
    FullBypass,

    /// Drop all messages received to the interface.
    DropAll,
}

pub struct MessageFilter<T: NetworkBackend> {
    iface_filters: HashMap<T::InterfaceId, FilterType>,
    iface_peers: HashMap<T::InterfaceId, HashSet<T::PeerId>>,
    iface_links: HashMap<T::InterfaceId, HashSet<T::InterfaceId>>,
    peer_filters: HashMap<(T::InterfaceId, T::PeerId), FilterType>,
}

impl<T: NetworkBackend> MessageFilter<T> {
    pub fn new() -> Self {
        Self {
            iface_filters: HashMap::new(),
            iface_peers: HashMap::new(),
            iface_links: HashMap::new(),
            peer_filters: HashMap::new(),
        }
    }

    /// Register interface to [`MessageFilter`].
    pub fn register_interface(
        &mut self,
        interface: T::InterfaceId,
        filter: FilterType,
    ) -> crate::Result<()> {
        ensure!(
            !self.iface_filters.contains_key(&interface),
            Error::InterfaceAlreadyExists,
        );
        ensure!(
            !self.iface_peers.contains_key(&interface),
            Error::InterfaceAlreadyExists,
        );

        tracing::info!(
            target: LOG_TARGET,
            interface_id = ?interface,
            filter = ?filter,
            "register interface",
        );

        self.iface_filters.insert(interface, filter);
        self.iface_peers.insert(interface, Default::default());
        self.iface_links.insert(interface, Default::default());
        Ok(())
    }

    /// Link interfaces together.
    pub fn link_interfaces(
        &mut self,
        first: T::InterfaceId,
        second: T::InterfaceId,
    ) -> crate::Result<()> {
        ensure!(
            self.iface_filters.contains_key(&first) && self.iface_filters.contains_key(&second),
            Error::InterfaceDoesntExist,
        );

        self.iface_links.entry(first).or_default().insert(second);
        self.iface_links.entry(second).or_default().insert(first);

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
            self.iface_filters.contains_key(&interface),
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
        self.iface_peers
            .get_mut(&interface)
            .expect("interface peers to exist")
            .insert(peer);
        Ok(())
    }

    /// Inject message into [`MessageFilter`].
    ///
    /// The message is processed based on the source peer and interface IDs and message type
    /// using any user-installed filters to further alter the message processing.
    ///
    /// After the processing is done, TODO:
    pub fn inject_message(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        message: &T::Message,
    ) -> crate::Result<(impl Iterator<Item = (T::InterfaceId, T::PeerId)>)> {
        ensure!(
            self.iface_filters.contains_key(&interface),
            Error::InterfaceDoesntExist,
        );

        let span = tracing::span!(target: LOG_TARGET, Level::INFO, "inject_message");
        let _guard = span.enter();

        tracing::event!(
            target: LOG_TARGET,
            Level::TRACE,
            peer_id = ?peer,
            interface_id = ?interface,
            message = ?message,
            "inject message",
        );

        // special case (TODO: refactor into something more sensible)
        if let FilterType::DropAll = self.iface_filters.get(&interface).expect("entry to exist") {
            return Ok(vec![].into_iter());
        }

        // TODO: this needs some serious thought
        let pairs = std::iter::once(&interface)
            .chain(
                self.iface_links
                    .get(&interface)
                    .expect("links to exist")
                    .iter(),
            )
            .filter_map(
                |iface| match self.iface_filters.get(iface).expect("filter to exist") {
                    FilterType::DropAll => None,
                    FilterType::FullBypass => Some(
                        self.iface_peers
                            .get(&iface)
                            .expect("interface peers to exist")
                            .iter()
                            .filter_map(|&iface_peer| {
                                if (iface_peer != peer && iface == &interface) {
                                    return Some((*iface, iface_peer));
                                } else if iface != &interface {
                                    return Some((*iface, iface_peer));
                                }

                                None
                            })
                            .collect::<Vec<_>>(),
                    ),
                },
            )
            .flatten()
            .collect::<Vec<_>>();

        tracing::event!(
            target: LOG_TARGET,
            Level::TRACE,
            peer_id = ?peer,
            interface_id = ?interface,
            pairs = ?pairs,
            "collected destinations for message"
        );

        Ok(pairs.into_iter())
    }
}
