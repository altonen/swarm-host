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
        .register_async_method("connect", |params, ctx| async move {
            let mut params = params.sequence();
            let address: String = params.next().expect("address");

            tracing::info!(
                target: LOG_TARGET,
                address = address,
                "connect to remote node"
            );

            let (tx, rx) = oneshot::channel();
            ctx.send(OverseerEvent::ConnectToPeer(address, tx))
                .await
                .expect("channel to stay open");
            match rx.await.expect("channel to stay open") {
                Ok(_) => Result::<_, jsonrpsee::core::Error>::Ok(()),
                Err(err) => Err(jsonrpsee::core::Error::Custom(err)),
            }
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

    module
        .register_async_method("get_local_address", |_params, ctx| async move {
            tracing::info!(target: LOG_TARGET, "get local address");

            let (tx, rx) = oneshot::channel();
            ctx.send(OverseerEvent::GetLocalAddress(tx))
                .await
                .expect("channel to stay open");
            Result::<_, jsonrpsee::core::Error>::Ok(rx.await.expect("channel to stay open"))
        })
        .unwrap();

    server.start(module).unwrap().stopped().await;
}
