use jsonrpsee::server::{RpcModule, ServerBuilder};

use std::net::SocketAddr;

const LOG_TARGET: &'static str = "rpc";

// TODO: convert into a struct

pub async fn run_server(address: SocketAddr) {
    tracing::debug!(
        target: LOG_TARGET,
        address = ?address,
        "starting rpc server"
    );

    let server = ServerBuilder::default().build(address).await.unwrap();
    let module = RpcModule::new(());

    server.start(module).unwrap().stopped().await;
}
