use crate::{
    backend::mockchain::{types::ProtocolId, MockchainBackend},
    executor::pyo3::PyO3Executor,
    filter::{tests::MockPacketSink, Filter},
    heuristics::HeuristicsBackend,
};

use rand::Rng;
use tokio::sync::mpsc;

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
    let (_backend, heuristics_handle) = HeuristicsBackend::new(None);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        tx,
        heuristics_handle,
    );

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_ok());
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
    let (_backend, heuristics_handle) = HeuristicsBackend::new(None);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        tx,
        heuristics_handle,
    );

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_err());
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
        .install_notification_filter(ProtocolId::Transaction, filter_code)
        .is_err());
}

#[test]
fn register_peer() {
    let filter_code = "
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

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(None);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        tx,
        heuristics_handle,
    );
    let mock_sink = Box::new(MockPacketSink::new());

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_ok());
    assert!(filter.register_peer(rng.gen(), mock_sink).is_ok());
}

#[test]
fn unregister_peer() {
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
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, _rx) = mpsc::channel(64);
    let peer = rng.gen();
    let interface = rng.gen();
    let (_backend, heuristics_handle) = HeuristicsBackend::new(None);
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
        interface,
        tx,
        heuristics_handle,
    );
    let mock_sink = Box::new(MockPacketSink::new());

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_ok());
    assert!(filter.register_peer(peer, mock_sink).is_ok());
    assert!(filter.unregister_peer(peer).is_ok());
    assert!(filter.unregister_peer(peer).is_err());
}
