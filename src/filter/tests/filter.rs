use crate::{
    backend::mockchain::{types::ProtocolId, MockchainBackend},
    executor::pyo3::PyO3Executor,
    filter::{tests::MockPacketSink, Filter},
    heuristics::HeuristicsBackend,
};

use rand::Rng;
use tokio::sync::mpsc;

use std::time::Duration;

#[test]
fn initialize_filter() {
    let filter_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(true);
    let (_, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        filter_code,
        Duration::from_millis(1000),
        tx,
        heuristics_handle,
    )
    .unwrap();
}

#[test]
fn context_initialization_function_missing() {
    let filter_code = "
def invalid_name_for_context_initialization_function(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(true);
    let result = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        filter_code,
        Duration::from_secs(1000),
        tx,
        heuristics_handle,
    );

    assert!(result.is_err());
}

#[test]
fn install_notification_filter() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def inject_notification(ctx, peer, notification):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(true);
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
}

#[test]
fn notification_filter_missing() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let filter_code = "
def __filter_notification__(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(true);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        context_code,
        Duration::from_millis(1000),
        tx,
        heuristics_handle,
    )
    .unwrap();

    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, filter_code)
        .is_err());
}

#[tokio::test]
async fn register_peer() {
    let filter_code = "
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

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(true);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        filter_code,
        Duration::from_millis(1000),
        tx,
        heuristics_handle,
    )
    .unwrap();
    let mock_sink = Box::new(MockPacketSink::new());

    assert!(filter.register_peer(rng.gen(), mock_sink).await.is_ok());
}

#[tokio::test]
async fn unregister_peer() {
    let filter_code = "
class Context():
    def __init__(self):
        self.peers = {}

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx, peer):
    print('register peer %d to filter' % (peer))
    ctx.peers[peer] = peer

def unregister_peer(ctx, peer):
    print('unregister peer %d from filter' % (peer))
    if peer in ctx.peers:
        del ctx.peers[peer]
    else:
        print('peer does not exist')

def poll(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let peer = rng.gen();
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(true);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        filter_code,
        Duration::from_millis(1000),
        tx,
        heuristics_handle,
    )
    .unwrap();
    let mock_sink = Box::new(MockPacketSink::new());

    assert!(filter.register_peer(peer, mock_sink).await.is_ok());
    assert!(filter.unregister_peer(peer).await.is_ok());
    assert!(filter.unregister_peer(peer).await.is_err());
}
