use crate::types::OverseerEvent;

use jsonrpsee::server::{RpcModule, ServerBuilder};
use tokio::sync::mpsc::Sender;

use std::net::{IpAddr, SocketAddr};

const LOG_TARGET: &'static str = "rpc";

pub async fn run_server(overseer_tx: Sender<OverseerEvent>, address: String, port: u16) {
    let server = ServerBuilder::default()
        .build(
            format!("{}:{}", address, port)
                .parse::<SocketAddr>()
                .unwrap(),
        )
        .await
        .unwrap();
    let mut module = RpcModule::new(overseer_tx);

    module
        .register_method("say_hello", |_, _| Ok("lo"))
        .unwrap();

    module
        .register_async_method("connect_to_peer", |params, ctx| async move {
            let mut params = params.sequence();
            let address: String = params.next().expect("addres");
            let port: u16 = params.next().expect("port");

            tracing::info!(
                target: LOG_TARGET,
                address = ?address,
                port = ?port,
                "attempt to establish connection to peer"
            );

            ctx.send(OverseerEvent::ConnectToPeer(address, port))
                .await
                .expect("channel to stay open");
            Result::<_, jsonrpsee::core::Error>::Ok("lo")
        })
        .unwrap();

    server.start(module).unwrap().stopped().await;
}
