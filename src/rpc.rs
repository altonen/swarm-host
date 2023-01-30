use jsonrpsee::server::{RpcModule, ServerBuilder};
use tokio::sync::{mpsc::Sender, oneshot};

use std::net::SocketAddr;

const LOG_TARGET: &'static str = "rpc";

pub async fn run_server(address: SocketAddr) {
    let server = ServerBuilder::default().build(address).await.unwrap();

    let mut module = RpcModule::new();
    server.start(module).unwrap().stopped().await;
}
