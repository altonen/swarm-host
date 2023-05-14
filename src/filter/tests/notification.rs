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

use std::time::Duration;

#[tokio::test]
async fn inject_notification() {
    let context_code = "
def initialize_ctx(ctx):
    pass

def poll(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def inject_notification(ctx, peer, protocol, notification):
    if peer is not None:
        print('peer %d' % (peer))
    print(notification)
    return { 'Drop': None }
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(false);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        context_code,
        Duration::from_millis(1000),
        tx,
        heuristics_handle,
    )
    .unwrap();

    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());
    assert!(filter
        .inject_notification(ProtocolId::Transaction, rng.gen(), rand::random())
        .await
        .is_ok());
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

def poll(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def inject_notification(ctx, peer, protocol, notification):
    if peer is not None:
        print('peer %d' % (peer))
    print(notification)
    return { 'Forward': None }
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(false);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        context_code,
        Duration::from_millis(1000),
        tx,
        heuristics_handle,
    )
    .unwrap();

    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());

    // register two peers and create packet sinks for them
    let peer1 = rng.gen();
    let peer2 = rng.gen();
    let (sink1, _msg_recv1, _, _) = DummyPacketSink::new();
    let (sink2, _msg_recv2, _, _) = DummyPacketSink::new();

    assert!(filter.register_peer(peer1, Box::new(sink1)).await.is_ok());
    assert!(filter.register_peer(peer2, Box::new(sink2)).await.is_ok());

    // inject the notification to a filter that delays it
    // verify that the notification is added to the list of delayed notifications
    let message: Message = rand::random();
    assert!(filter
        .inject_notification(ProtocolId::Transaction, rng.gen(), message.clone())
        .await
        .is_ok());
}
