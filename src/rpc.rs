#![allow(unused)]

use crate::types::OverseerEvent;

use jsonrpsee::server::{RpcModule, ServerBuilder};
use tokio::sync::{mpsc::Sender, oneshot};

use std::net::SocketAddr;

const LOG_TARGET: &'static str = "rpc";

// TODO: convert into a struct

pub async fn run_server(overseer_tx: Sender<OverseerEvent>, address: SocketAddr) {
    tracing::debug!(
        target: LOG_TARGET,
        address = ?address,
        "starting rpc server"
    );

    let server = ServerBuilder::default().build(address).await.unwrap();
    let mut module = RpcModule::new(());

    module
        .register_async_method("create_interface", |params, ctx| async move {
            let mut params = params.sequence();
            let address: String = params.next().expect("address");

            tracing::trace!(
                target: LOG_TARGET,
                address = address,
                "received call to create new interface"
            );

            // let (tx, rx) = oneshot::channel();
            // ctx.send(OverseerEvent::CreateInterface { result: tx })
            //     .await
            //     .expect("channel to stay open");

            Result::<_, jsonrpsee::core::Error>::Ok(())
        })
        .unwrap();

    server.start(module).unwrap().stopped().await;
}
