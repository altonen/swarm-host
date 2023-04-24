use crate::{
    backend::mockchain::{
        types::{Message, ProtocolId},
        MockchainBackend,
    },
    executor::pyo3::PyO3Executor,
    filter::{tests::DummyPacketSink, Filter},
    heuristics::HeuristicsBackend,
};

use rand::Rng;
use tokio::sync::mpsc;

#[tokio::test]
async fn inject_notification() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def filter_notification(ctx, peer, notification):
    if peer is not None:
        print('peer %d' % (peer))
    print(notification)
    return { 'Drop': None }
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(None);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        tx,
        heuristics_handle,
    );

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());
    assert!(filter
        .inject_notification(&ProtocolId::Transaction, rng.gen(), rand::random())
        .await
        .is_ok());
}

#[tokio::test]
async fn delay_notification() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def filter_notification(ctx, peer, notification):
    if peer is not None:
        print('peer %d' % (peer))
    print(notification)
    return { 'Delay': 15 }
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(None);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        tx,
        heuristics_handle,
    );

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());

    // inject the notification to a filter that delays it
    // verify that the notification is added to the list of delayed notifications
    assert!(filter.delayed_notifications.is_empty());
    assert!(filter
        .inject_notification(&ProtocolId::Transaction, rng.gen(), rand::random())
        .await
        .is_ok());
    assert_eq!(filter.delayed_notifications.len(), 1);
}

#[tokio::test]
async fn inject_notification_and_forward_it() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let context_code = "
class Context():
    def __init__(self):
        self.peers = {}

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx, peer):
    print('register peer %d to filter' % (peer))
    ctx.peers[peer] = peer
    "
    .to_owned();
    let notification_filter_code = "
def filter_notification(ctx, peer, notification):
    if peer is not None:
        print('peer %d' % (peer))
    print(notification)
    return { 'Forward': None }
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(None);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        tx,
        heuristics_handle,
    );

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());

    // register two peers and create packet sinks for them
    let peer1 = rng.gen();
    let peer2 = rng.gen();
    let (sink1, mut msg_recv1, _, _) = DummyPacketSink::new();
    let (sink2, mut msg_recv2, _, _) = DummyPacketSink::new();

    assert!(filter.register_peer(peer1, Box::new(sink1)).is_ok());
    assert!(filter.register_peer(peer2, Box::new(sink2)).is_ok());

    // inject the notification to a filter that delays it
    // verify that the notification is added to the list of delayed notifications
    let message: Message = rand::random();
    assert!(filter.delayed_notifications.is_empty());
    assert!(filter
        .inject_notification(&ProtocolId::Transaction, rng.gen(), message.clone())
        .await
        .is_ok());

    assert_eq!(msg_recv1.try_recv(), Ok(message.clone()));
    assert_eq!(msg_recv2.try_recv(), Ok(message));
}
