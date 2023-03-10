use crate::{
    backend::mockchain::{
        types::{InterfaceId, Message, PeerId, Transaction},
        MockchainBackend,
    },
    filter::{FilterType, LinkType, MessageFilter},
};

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

    filter
        .install_notification_filter(0usize, Box::new(msg_filter))
        .unwrap();
    filter
        .install_notification_filter(1usize, Box::new(msg_filter))
        .unwrap();

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
