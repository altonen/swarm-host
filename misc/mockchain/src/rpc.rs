use crate::types::{OverseerEvent, PeerId};

use futures::{future, prelude::*};
use rand::{
    distributions::{Distribution, Uniform},
    thread_rng,
};
use tarpc::{
    context,
    server::{self, incoming::Incoming, Channel},
    tokio_serde::formats::Json,
};
use tokio::{sync::mpsc::Sender, time};

use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

const LOG_TARGET: &'static str = "rpc";

#[tarpc::service]
pub trait World {
    /// Returns a greeting for name.
    async fn hello(name: String) -> String;

    // /// Connect to peer.
    // async fn connect_to_peer(address: String, port: u16);
}

#[derive(Clone)]
struct HelloServer {
    address: SocketAddr,
    overseer_tx: Sender<OverseerEvent>,
}

#[tarpc::server]
impl World for HelloServer {
    async fn hello(self, _: context::Context, name: String) -> String {
        let sleep_time =
            Duration::from_millis(Uniform::new_inclusive(1, 10).sample(&mut thread_rng()));
        time::sleep(sleep_time).await;
        format!("Hello, {name}! You are connected from {}", self.address)
    }

    // async fn connect_to_peer(self, _: context::Context, address: String, port: u16) {
    //     tracing::info!(
    //         target: LOG_TARGET,
    //         address = ?address,
    //         "attempt to establish connection to peer"
    //     );

    //     self.overseer_tx
    //         .send(OverseerEvent::ConnectToPeer(address, port))
    //         .await
    //         .expect("channel to stay open");

    //     // TODO: send `oneshot::Sender` and wait for result here
    // }
}

pub async fn run_server(
    overseer_tx: Sender<OverseerEvent>,
    server_addr: (IpAddr, u16),
) -> anyhow::Result<()> {
    let mut listener = tarpc::serde_transport::tcp::listen(&server_addr, Json::default).await?;
    tracing::info!("Listening on port {}", listener.local_addr().port());

    listener.config_mut().max_frame_length(usize::MAX);
    listener
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        .max_channels_per_key(1, |t| t.transport().peer_addr().unwrap().ip())
        .map(|channel| {
            tracing::info!(target: LOG_TARGET, "start serving RCP request",);

            let server = HelloServer {
                address: channel.transport().peer_addr().unwrap(),
                overseer_tx: overseer_tx.clone(),
            };
            channel.execute(server.serve())
        })
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    Ok(())
}
