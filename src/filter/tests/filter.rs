use crate::{
    backend::mockchain::{types::ProtocolId, MockchainBackend},
    error::Error,
    executor::pyo3::PyO3Executor,
    filter::Filter,
};

use rand::Rng;
use tokio::sync::mpsc;

#[tokio::test]
async fn initialize_filter() {
    // TODO: move somewhere else?
    let filter_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_ok());
}

#[tokio::test]
async fn context_initialization_function_missing() {
    // TODO: move somewhere else?
    let filter_code = "
def invalid_name_for_context_initialization_function(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_err());
}

#[tokio::test]
async fn install_notification_filter() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let notification_filter_code = "
def filter_notification(ctx, peer, notification):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, notification_filter_code)
        .is_ok());
}

#[tokio::test]
async fn notification_filter_missing() {
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
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, filter_code)
        .is_err());
}

#[tokio::test]
async fn invalid_signature_for_filter_notification() {
    let context_code = "
def initialize_ctx(ctx):
    pass
    "
    .to_owned();
    let filter_code = "
def filter_notification(ctx):
    pass
    "
    .to_owned();

    let mut rng = rand::thread_rng();
    let (tx, rx) = mpsc::channel(64);
    let interface = rng.gen();
    let (mut filter, _) = Filter::<MockchainBackend, PyO3Executor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, context_code, None)
        .is_ok());
    assert!(filter
        .install_notification_filter(ProtocolId::Transaction, filter_code)
        .is_err());
}
