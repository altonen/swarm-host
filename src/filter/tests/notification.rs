use crate::{
    backend::mockchain::{
        types::{InterfaceId, Message, PeerId, ProtocolId, Transaction},
        MockchainBackend,
    },
    filter::{FilterType, LinkType, MessageFilter},
};

// inject message into filter and sort the resulting (interface, peer) pairs before returning them
fn inject_and_sort_results(
    filter: &mut MessageFilter<MockchainBackend>,
    interface: usize,
    peer: u64,
    protocol: &ProtocolId,
    message: &Message,
) -> Vec<(InterfaceId, PeerId)> {
    let mut messages = filter
        .inject_notification(interface, peer, protocol, message)
        .expect("valid configuration")
        .collect::<Vec<_>>();

    messages.sort_by(|a, b| {
        if a.0 == b.0 {
            a.1.cmp(&b.1)
        } else {
            a.0.cmp(&b.0)
        }
    });
    messages
}

// censor transactions from peer `1337`
#[test]
fn custom_message_filter() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    // TODO: move to separate file for better indentation and readability?
    // TODO: shold take context?
    let filter_code = "
def filter_notification(
    src_interface,
    src_peer,
    dst_interface,
    dst_peer,
    protocol,
    notification
):
    if src_peer == 1337:
        return False
    return True
    ";
    let protocol_name = ProtocolId::Transaction;

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

    // register `peer0` to `iface0` and `peer1337` and `peer1338` to `iface1`
    filter
        .register_peer(0usize, 0u64, FilterType::FullBypass)
        .unwrap();
    filter
        .register_peer(1usize, 1337u64, FilterType::FullBypass)
        .unwrap();
    filter
        .register_peer(1usize, 1338u64, FilterType::FullBypass)
        .unwrap();

    // inject message to first interface and verify it's forwarded to the other interface
    assert_eq!(
        inject_and_sort_results(&mut filter, 0usize, 0u64, &protocol_name, &rand::random()),
        vec![(1usize, 1337u64), (1usize, 1338u64)]
    );

    filter
        .install_notification_filter(0usize, protocol_name, filter_code.to_owned())
        .unwrap();

    // random valid message is forwarded
    assert_eq!(
        inject_and_sort_results(
            &mut filter,
            1usize,
            1338u64,
            &protocol_name,
            &rand::random()
        ),
        vec![(0usize, 0u64), (1usize, 1337u64)],
    );

    // transaction from peer `1337` is not forwarded to `iface0`
    assert_eq!(
        inject_and_sort_results(
            &mut filter,
            1usize,
            1337u64,
            &protocol_name,
            &rand::random()
        ),
        vec![(1usize, 1338u64)],
    );
}
