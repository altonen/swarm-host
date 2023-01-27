use crate::types::{OverseerEvent, PeerId};

use jsonrpsee::server::{RpcModule, ServerBuilder};
use tokio::sync::{mpsc::Sender, oneshot};

use std::net::SocketAddr;

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
            Result::<_, jsonrpsee::core::Error>::Ok("")
        })
        .unwrap();

    module
        .register_async_method("disconnect_peer", |params, ctx| async move {
            let mut params = params.sequence();
            let peer: PeerId = params.next().expect("peer id");

            tracing::info!(target: LOG_TARGET, id = peer, "disconnect peer");

            ctx.send(OverseerEvent::DisconnectPeer(peer))
                .await
                .expect("channel to stay open");
            Result::<_, jsonrpsee::core::Error>::Ok("")
        })
        .unwrap();

    module
        .register_async_method("get_local_peer_id", |_params, ctx| async move {
            tracing::info!(target: LOG_TARGET, "get local peer id");

            let (tx, rx) = oneshot::channel();
            ctx.send(OverseerEvent::GetLocalPeerId(tx))
                .await
                .expect("channel to stay open");
            Result::<_, jsonrpsee::core::Error>::Ok(rx.await.expect("channel to stay open"))
        })
        .unwrap();

    server.start(module).unwrap().stopped().await;
}
