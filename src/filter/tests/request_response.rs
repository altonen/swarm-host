use crate::{
    backend::mockchain::{
        types::{
            Block, BlockRequest, BlockResponse, InterfaceId, Message, PeerId, ProtocolId, Request,
            RequestId, Response, Transaction,
        },
        MockchainBackend,
    },
    executor::pyo3::PyO3Executor,
    filter::{tests::DummyPacketSink, Filter},
};

use parity_scale_codec::{Decode, Encode};
use rand::Rng;
use tokio::sync::mpsc;

use std::{env, fs, path::PathBuf};

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
async fn inject_request_for_blocks_that_dont_exist() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let context_code = "
import redis

class Context():
    def __init__(self):
        self.peers = {}
        self.pending_requests = {}
        self.database = redis.Redis(host = 'localhost', port = 6379, decode_responses = True)
        self.database.ping()
        self.database.flushall()

class PeerContext():
    def __init__(self):
        self.best_block = 555
        self.pending_request = False

def initialize_ctx(ctx):
    return Context()

def register_peer(ctx, peer):
    ctx.peers[peer] = PeerContext()
    "
    .to_owned();

    let mut file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    file_path.push("src/filter/tests/filters/inject_request_and_response.py");
    let request_response_filter_code = fs::read_to_string(file_path).unwrap();

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
    let (sink1, mut msg_recv1, mut req_recv1, mut resp_recv1) = DummyPacketSink::new();
    let (sink2, mut msg_recv2, mut req_recv2, mut resp_recv2) = DummyPacketSink::new();

    assert!(filter.register_peer(peer1, Box::new(sink1)).is_ok());
    assert!(filter.register_peer(peer2, Box::new(sink2)).is_ok());

    assert!(filter
        .inject_request(
            &ProtocolId::BlockRequest,
            peer1,
            Request::new(RequestId(1337), BlockRequest::new(123u128, 16u8).encode()),
        )
        .await
        .is_ok());

    // verify that the request is received by the other peer
    assert!(std::matches!(req_recv2.try_recv(), Ok(_)));

    let sent_response = BlockResponse::new(vec![Block::from_transactions(
        123u128,
        (0..1).map(|_| rand::random()).collect::<Vec<_>>(),
    )]);
    assert!(filter
        .inject_response(
            &ProtocolId::BlockRequest,
            peer2,
            Response::new(RequestId(1337), sent_response.encode()),
        )
        .await
        .is_ok());

    let mut response = resp_recv1.try_recv().unwrap();
    let response: BlockResponse = Decode::decode(&mut &response[..]).unwrap();
    assert_eq!(response, sent_response);

    assert!(filter
        .inject_request(
            &ProtocolId::BlockRequest,
            peer1,
            Request::new(RequestId(1337), BlockRequest::new(123u128, 16u8).encode()),
        )
        .await
        .is_ok());
    let mut response = resp_recv1.try_recv().unwrap();
    let response: BlockResponse = Decode::decode(&mut &response[..]).unwrap();
}
