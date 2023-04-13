use crate::{
    backend::mockchain::{
        types::{InterfaceId, Message, PeerId, ProtocolId, Transaction},
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
def filter_request(ctx, peer, notification):
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
def filter_response(ctx, peer, notification):
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
