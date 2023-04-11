use crate::{
    backend::mockchain::MockchainBackend, error::Error, executor::python::PythonExecutor,
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
    let (mut filter, _) = Filter::<MockchainBackend, PythonExecutor>::new(interface, tx);

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
    let (mut filter, _) = Filter::<MockchainBackend, PythonExecutor>::new(interface, tx);

    assert!(filter
        .initialize_filter(interface, filter_code, None)
        .is_err());
}
