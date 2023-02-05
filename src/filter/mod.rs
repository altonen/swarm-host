//! Message filtering implementation.

use crate::{
    backend::{InterfaceType, NetworkBackend},
    ensure,
    error::Error,
};

use tracing::Level;

use std::collections::{HashMap, HashSet};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mockchain::{types::Message, InterfaceId, MockchainBackend};
    use rand::Rng;

    #[test]
    fn register_new_interface() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        assert_eq!(
            filter.register_interface(0usize, FilterType::FullBypass),
            Ok(())
        );
        assert_eq!(
            filter.iface_filters.get(&0usize),
            Some(&FilterType::FullBypass),
        );
        assert_eq!(
            filter.register_interface(0usize, FilterType::FullBypass),
            Err(Error::InterfaceAlreadyExists),
        );
    }

    #[test]
    fn duplicate_interface() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        assert_eq!(
            filter.register_interface(0usize, FilterType::FullBypass),
            Ok(())
        );
        assert_eq!(
            filter.register_peer(0usize, 0u64, FilterType::FullBypass),
            Ok(())
        );
        assert_eq!(
            filter.register_peer(0usize, 0u64, FilterType::FullBypass),
            Err(Error::PeerAlreadyExists),
        );
    }

    #[test]
    fn duplicate_peer() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        assert_eq!(
            filter.register_interface(0usize, FilterType::FullBypass),
            Ok(())
        );
    }

    #[test]
    fn unknown_interface_for_peer() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        assert_eq!(
            filter.register_peer(0usize, 0u64, FilterType::FullBypass),
            Err(Error::InterfaceDoesntExist),
        );
    }

    #[test]
    fn inject_message_unknown_interface() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        if let Err(err) = filter.inject_message(0usize, 0u64, &rand::random()) {
            assert_eq!(err, Error::InterfaceDoesntExist);
        } else {
            panic!("invalid response from `inject_message()`");
        }
    }

    #[test]
    fn interface_full_bypass_two_peers_one_interface() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        assert_eq!(
            filter.register_interface(0usize, FilterType::FullBypass),
            Ok(())
        );
        assert_eq!(
            filter.register_peer(0usize, 0u64, FilterType::FullBypass),
            Ok(())
        );
        assert_eq!(
            filter.register_peer(0usize, 1u64, FilterType::FullBypass),
            Ok(())
        );

        assert_eq!(
            filter
                .inject_message(0usize, 0u64, &rand::random())
                .expect("valid configuration")
                .collect::<Vec<_>>(),
            vec![(0usize, 1u64)],
        );
    }

    #[test]
    fn interface_full_bypass_n_peers_one_interface() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut rng = rand::thread_rng();
        let mut filter = MessageFilter::<MockchainBackend>::new();

        assert_eq!(
            filter.register_interface(0usize, FilterType::FullBypass),
            Ok(())
        );

        let upper_bound = rng.gen_range(2..10);
        let selected_peer = rng.gen_range(0..upper_bound);

        for i in (0..upper_bound) {
            assert_eq!(
                filter.register_peer(0usize, i, FilterType::FullBypass),
                Ok(())
            );
        }

        let peers = filter
            .inject_message(0usize, selected_peer, &rand::random())
            .expect("valid configuration")
            .collect::<HashSet<_>>();

        assert_eq!(
            peers.len(),
            TryInto::<usize>::try_into(upper_bound - 1).unwrap()
        );
        assert!(!peers.contains(&(0usize, selected_peer)));
    }

    #[test]
    fn interface_drop() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();

        assert_eq!(
            filter.register_interface(0usize, FilterType::DropAll),
            Ok(())
        );
        assert_eq!(
            filter.iface_filters.get(&0usize),
            Some(&FilterType::DropAll),
        );
        assert_eq!(
            filter.register_peer(0usize, 0u64, FilterType::FullBypass),
            Ok(())
        );
        assert_eq!(
            filter.register_peer(0usize, 1u64, FilterType::FullBypass),
            Ok(())
        );
        assert_eq!(
            filter
                .inject_message(0usize, 0u64, &rand::random())
                .expect("valid configuration")
                .collect::<Vec<_>>(),
            vec![],
        );
    }

    #[test]
    fn two_interfaces_no_link() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        filter
            .register_interface(0usize, FilterType::FullBypass)
            .unwrap();
        filter
            .register_interface(1usize, FilterType::FullBypass)
            .unwrap();

        // register `peer0` to `iface0` and `peer1` to `iface1`
        filter
            .register_peer(0usize, 0u64, FilterType::FullBypass)
            .unwrap();
        filter
            .register_peer(1usize, 1u64, FilterType::FullBypass)
            .unwrap();

        // because interfaces are not linked, injecting a message to `iface0`
        // doesn't get forwarded to `peer1` because it's on a different different interface.
        assert_eq!(
            filter
                .inject_message(0usize, 0u64, &rand::random())
                .expect("valid configuration")
                .collect::<Vec<_>>(),
            vec![],
        );
    }

    #[test]
    fn two_linked_interfaces() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        filter
            .register_interface(0usize, FilterType::FullBypass)
            .unwrap();
        filter
            .register_interface(1usize, FilterType::FullBypass)
            .unwrap();

        // link interfaces together so messages can flow between them
        filter.link_interfaces(0usize, 1usize).unwrap();
        assert_eq!(
            filter.iface_links.get(&0usize),
            Some(&HashSet::from([1usize])),
        );
        assert_eq!(
            filter.iface_links.get(&1usize),
            Some(&HashSet::from([0usize])),
        );

        // register `peer0` to `iface0` and `peer1` to `iface1`
        filter
            .register_peer(0usize, 0u64, FilterType::FullBypass)
            .unwrap();
        filter
            .register_peer(1usize, 1u64, FilterType::FullBypass)
            .unwrap();

        // inject message to first interface and verify it's forwarded to the other interface
        assert_eq!(
            filter
                .inject_message(0usize, 0u64, &rand::random())
                .expect("valid configuration")
                .collect::<Vec<_>>(),
            vec![(1usize, 1u64)],
        );
    }

    #[test]
    fn peer_connected_to_two_linked_interfaces_receives() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        filter
            .register_interface(0usize, FilterType::FullBypass)
            .unwrap();
        filter
            .register_interface(1usize, FilterType::FullBypass)
            .unwrap();

        // link interfaces together so messages can flow between them
        filter.link_interfaces(0usize, 1usize).unwrap();
        assert_eq!(
            filter.iface_links.get(&0usize),
            Some(&HashSet::from([1usize])),
        );
        assert_eq!(
            filter.iface_links.get(&1usize),
            Some(&HashSet::from([0usize])),
        );

        // register `peer0` to `iface0` and `iface1`
        filter
            .register_peer(0usize, 0u64, FilterType::FullBypass)
            .unwrap();
        filter
            .register_peer(1usize, 0u64, FilterType::FullBypass)
            .unwrap();

        // inject message to first interface and verify it's forwarded to the other interface
        assert_eq!(
            filter
                .inject_message(0usize, 0u64, &rand::random())
                .expect("valid configuration")
                .collect::<Vec<_>>(),
            vec![(1usize, 0u64)],
        );
    }

    #[test]
    fn peer_connected_to_two_unlinked_interfaces() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        filter
            .register_interface(0usize, FilterType::FullBypass)
            .unwrap();
        filter
            .register_interface(1usize, FilterType::FullBypass)
            .unwrap();

        // register `peer0` to `iface0` and `iface1`
        filter
            .register_peer(0usize, 0u64, FilterType::FullBypass)
            .unwrap();
        filter
            .register_peer(1usize, 0u64, FilterType::FullBypass)
            .unwrap();

        // inject message to first interface and verify it's forwarded to the other interface
        assert_eq!(
            filter
                .inject_message(0usize, 0u64, &rand::random())
                .expect("valid configuration")
                .collect::<Vec<_>>(),
            vec![],
        );
    }

    #[test]
    fn linked_interfaces_with_dropall() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut filter = MessageFilter::<MockchainBackend>::new();
        filter
            .register_interface(0usize, FilterType::FullBypass)
            .unwrap();
        filter
            .register_interface(1usize, FilterType::DropAll)
            .unwrap();

        // link interfaces together so messages can flow between them
        filter.link_interfaces(0usize, 1usize).unwrap();
        assert_eq!(
            filter.iface_links.get(&0usize),
            Some(&HashSet::from([1usize])),
        );
        assert_eq!(
            filter.iface_links.get(&1usize),
            Some(&HashSet::from([0usize])),
        );

        // register `peer0` to `iface0` and `iface1`
        filter
            .register_peer(0usize, 0u64, FilterType::FullBypass)
            .unwrap();
        filter
            .register_peer(1usize, 0u64, FilterType::FullBypass)
            .unwrap();

        // inject message to first interface and verify that it is not received
        // by the second interface because its type is `InterfaceType::DropAll`
        assert_eq!(
            filter
                .inject_message(0usize, 0u64, &rand::random())
                .expect("valid configuration")
                .collect::<Vec<_>>(),
            vec![],
        );
    }

    // chain `N` interfaces together,
    // publish message at the head and verify it's received by the tail
    // #[test]
    // fn chained_interfaces() {
    //     tracing_subscriber::fmt()
    //         .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    //         .try_init();

    //     let mut rng = rand::thread_rng();
    //     let mut filter = MessageFilter::<MockchainBackend>::new();

    //     // let upper_bound = rng.gen_range(2..10);
    //     let upper_bound = 3;
    //     let selected = rng.gen_range(0..upper_bound);

    //     for i in (0..upper_bound) {
    //         assert_eq!(filter.register_interface(i, FilterType::FullBypass), Ok(()));
    //         assert_eq!(
    //             filter.register_peer(
    //                 i,
    //                 i.try_into().unwrap(),
    //                 FilterType::FullBypass
    //             ),
    //             Ok(())
    //         );

    //         if i > 0 {
    //             filter.link_interfaces(i - 1, i).unwrap();
    //         }
    //     }

    //     let forwards = filter
    //         .inject_message(
    //             selected,
    //             selected.try_into().unwrap(),
    //             &rand::random(),
    //         )
    //         .expect("valid configuration")
    //         .collect::<HashSet<_>>();

    //     assert_eq!(
    //         forwards.len(),
    //         TryInto::<usize>::try_into(upper_bound - 1).unwrap()
    //     );
    //     // assert!(!peers.contains(&(0usize, selected.try_into().unwrap())));
    // }
}
