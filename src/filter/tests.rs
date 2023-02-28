use super::*;
use crate::backend::mockchain::{
    types::{InterfaceId, Message, PeerId, Transaction},
    MockchainBackend,
};
use rand::Rng;

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
            .inject_message(0usize, 0u64, &rand::random())
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
            .inject_message(0usize, 0u64, &rand::random())
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![(1usize, 1u64)],
    );

    // unlink interfaces
    filter.unlink_interface(0usize, 1usize).unwrap();

    // verify that messages don't flow between the interfaces anymore
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
        .inject_message(selected, selected.try_into().unwrap(), &rand::random())
        .expect("valid configuration")
        .collect::<HashSet<_>>();

    assert_eq!(
        forwards.len(),
        TryInto::<usize>::try_into(upper_bound - 1).unwrap()
    );
    assert!(!forwards.contains(&(0usize, selected.try_into().unwrap())));
}

#[test]
fn custom_message_filter() {
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
            .inject_message(0usize, 0u64, &rand::random())
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![(1usize, 1u64)],
    );

    // add custom message filter
    let msg_filter = |src_iface: InterfaceId,
                      src_peer: PeerId,
                      dst_iface: InterfaceId,
                      dst_peer: PeerId,
                      message: &Message| {
        if src_peer == 1337u64 {
            return false;
        }

        if let &Message::Transaction(tx) = &message {
            if tx.receiver() == 1337u64 && tx.amount() >= 10_000 {
                return false;
            }
        }

        true
    };

    filter.add_filter(0usize, Box::new(msg_filter)).unwrap();
    filter.add_filter(1usize, Box::new(msg_filter)).unwrap();

    // random valid message is forwarded
    assert_eq!(
        filter
            .inject_message(
                0usize,
                0u64,
                &Message::Transaction(Transaction::new(1, 2, 3))
            )
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![(1usize, 1u64)],
    );

    // message from peer `1337` is not forwarded
    assert_eq!(
        filter
            .inject_message(0usize, 1337u64, &rand::random())
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![],
    );

    // transaction to peer `1337` if amount is under `10_00` is forwarded
    assert_eq!(
        filter
            .inject_message(
                0usize,
                0u64,
                &Message::Transaction(Transaction::new(1, 1337u64, 3))
            )
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![(1usize, 1u64)],
    );

    // transaction to peer `1337` if amount is over `10_00` is not forwarded
    assert_eq!(
        filter
            .inject_message(
                0usize,
                0u64,
                &Message::Transaction(Transaction::new(1, 1337u64, 10_001))
            )
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![],
    );
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
            .inject_message(0usize, 0u64, &rand::random())
            .expect("valid configuration")
            .collect::<Vec<_>>(),
        vec![(0usize, 1u64)],
    );
}
