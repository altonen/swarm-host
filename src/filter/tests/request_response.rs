use crate::{
    backend::mockchain::{
        types::{InterfaceId, Message, PeerId, ProtocolId, Request, Response, Transaction},
        MockchainBackend,
    },
    executor::pyo3::PyO3Executor,
    filter::{tests::DummyPacketSink, Filter},
};

use rand::Rng;
use tokio::sync::mpsc;

#[tokio::test]
async fn filter_response_missing() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def filter_request(ctx, peer, request):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) =
        Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_request_response_filter(ProtocolId::Transaction, notification_filter_code)
        .is_err());
}

#[tokio::test]
async fn filter_request_missing() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def filter_response(ctx, peer, response):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) =
        Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_request_response_filter(ProtocolId::Transaction, notification_filter_code)
        .is_err());
}

#[tokio::test]
async fn inject_request_and_todo() {
    tracing_subscriber::fmt()
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
    let request_response_filter_code = "
def filter_request(ctx, peer, request):
    return { 'DoNothing' : None }

def filter_response(ctx, peer, response):
    return { 'DoNothing' : None }
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) =
        Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_request_response_filter(ProtocolId::BlockRequest, request_response_filter_code)
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
    let peer = rng.gen();

    assert!(filter
        .inject_request(
            &ProtocolId::BlockRequest,
            peer,
            Request::new(1337, vec![11, 22, 33, 44])
        )
        .await
        .is_ok());
    assert!(filter
        .inject_response(
            &ProtocolId::BlockRequest,
            peer,
            Response::new(1337, vec![11, 22, 33, 44])
        )
        .await
        .is_ok());
}
