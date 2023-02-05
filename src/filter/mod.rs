//! Message filtering implementation.

use crate::{
    backend::{InterfaceType, NetworkBackend},
    ensure,
    error::Error,
};

use tracing::Level;

use std::collections::{hash_map::Entry, HashMap, HashSet};

#[cfg(test)]
mod tests;

const LOG_TARGET: &'static str = "filter";

// TODO: filter should have apply method?
// TODO: separate filter types for peers and interfaces
// TODO: more complex filters?
// TODO: differentiate between ingress and egress
// TODO: start using `mockall`
// TODO: documentation
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

struct Interface<T: NetworkBackend> {
    filter: FilterType,
    peers: HashSet<T::PeerId>,
    links: HashSet<T::InterfaceId>,
}

pub struct MessageFilter<T: NetworkBackend> {
    peer_filters: HashMap<(T::InterfaceId, T::PeerId), FilterType>,
    interfaces: HashMap<T::InterfaceId, Interface<T>>,
}

impl<T: NetworkBackend> MessageFilter<T> {
    pub fn new() -> Self {
        Self {
            peer_filters: HashMap::new(),
            interfaces: HashMap::new(),
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
                    peers: Default::default(),
                    links: Default::default(),
                });

                Ok(())
            }
        }
    }

    /// Link interfaces together.
    pub fn link_interfaces(
        &mut self,
        first: T::InterfaceId,
        second: T::InterfaceId,
    ) -> crate::Result<()> {
        ensure!(
            self.interfaces.contains_key(&first) && self.interfaces.contains_key(&second),
            Error::InterfaceDoesntExist,
        );

        self.interfaces
            .get_mut(&first)
            .expect("entry to exist")
            .links
            .insert(second);
        self.interfaces
            .get_mut(&second)
            .expect("entry to exist")
            .links
            .insert(first);

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
            self.interfaces.contains_key(&interface),
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
        if let FilterType::DropAll = self
            .interfaces
            .get(&interface)
            .expect("entry to exist")
            .filter
        {
            return Ok(vec![].into_iter());
        }

        // TODO: this needs some serious thought
        let pairs = std::iter::once(&interface)
            .chain(
                self.interfaces
                    .get(&interface)
                    .expect("entry to exist")
                    .links
                    .iter(),
            )
            .filter_map(
                |iface| match self.interfaces.get(iface).expect("filter to exist").filter {
                    FilterType::DropAll => None,
                    FilterType::FullBypass => Some(
                        self.interfaces
                            .get(&iface)
                            .expect("entry to exist")
                            .peers
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
