use crate::{
    backend::mockchain::{
        types::{InterfaceId, Message, PeerId, ProtocolId, Transaction},
        MockchainBackend,
    },
    error::Error,
    filter::{FilterType, LinkType, MessageFilter},
};

use rand::Rng;

use std::collections::HashSet;

// TODO: add tests `IngressOnly` and `EgressOnly`

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
        filter.interfaces.get(&0usize).unwrap().filter,
        FilterType::FullBypass,
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
fn inject_notification_unknown_interface() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut filter = MessageFilter::<MockchainBackend>::new();
    if let Err(err) =
        filter.inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
    {
        assert_eq!(err, Error::InterfaceDoesntExist);
    } else {
        panic!("invalid response from `inject_notification()`");
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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
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
        .inject_notification(
            0usize,
            selected_peer,
            &ProtocolId::Transaction,
            &rand::random(),
        )
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
        filter.interfaces.get(&0usize).unwrap().filter,
        FilterType::DropAll,
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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
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
    filter
        .link_interface(0usize, 1usize, LinkType::Bidrectional)
        .unwrap();

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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
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
    filter
        .link_interface(0usize, 1usize, LinkType::Bidrectional)
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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
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
    filter
        .link_interface(0usize, 1usize, LinkType::Bidrectional)
        .unwrap();

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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![],
    );
}

#[test]
fn link_message_unlink() {
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
    filter
        .link_interface(0usize, 1usize, LinkType::Bidrectional)
        .unwrap();

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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![(1usize, 1u64)],
    );

    // unlink interfaces
    filter.unlink_interface(0usize, 1usize).unwrap();

    // verify that messages don't flow between the interfaces anymore
    assert_eq!(
        filter
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![],
    );
}

// chain `N` interfaces together,
// publish message at the head and verify it's received by the tail
#[test]
fn chained_interfaces() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut rng = rand::thread_rng();
    let mut filter = MessageFilter::<MockchainBackend>::new();

    let upper_bound = rng.gen_range(2..10);
    let selected = rng.gen_range(0..upper_bound);

    for i in (0..upper_bound) {
        assert_eq!(filter.register_interface(i, FilterType::FullBypass), Ok(()));
        assert_eq!(
            filter.register_peer(i, i.try_into().unwrap(), FilterType::FullBypass),
            Ok(())
        );

        if i > 0 {
            filter
                .link_interface(i - 1, i, LinkType::Bidrectional)
                .unwrap();
        }
    }

    let forwards = filter
        .inject_notification(
            selected,
            selected.try_into().unwrap(),
            &ProtocolId::Transaction,
            &rand::random(),
        )
        .expect("valid configuration")
        .collect::<HashSet<_>>();

    assert_eq!(
        forwards.len(),
        TryInto::<usize>::try_into(upper_bound - 1).unwrap()
    );
    assert!(!forwards.contains(&(0usize, selected.try_into().unwrap())));
}

#[test]
fn test_function() {
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
            .inject_notification(0usize, 0u64, &ProtocolId::Transaction, &rand::random())
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![(0usize, 1u64)],
    );
}