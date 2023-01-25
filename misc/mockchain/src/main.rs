use crate::types::{Command, Message, OverseerEvent};

use clap::Parser;
use tokio::{net::TcpListener, sync::mpsc};
use tracing_subscriber::{fmt::format::FmtSpan, prelude::*};

use std::net::{IpAddr, Ipv6Addr};

const LOG_TARGET: &'static str = "overseer";

// TODO: mitä kaikkea täytyy olla implmentoituna:
//   - bind to interface
//   - publish block
//   - publish transaction
//   - connect to peer
//
//   - disconnect from peer
//   - query if tx is present
//   - query if block is present
//   - request block from peer
//   - create block and publish it
//   - submit transaction
//
// TODO: figure out architecture for the project
//   - simple event loop with timers?
//
// TODO: bind to interface instead

mod chainstate;
mod p2p;
mod rpc;
mod types;

#[derive(Parser)]
struct Flags {
    /// RPC port.
    #[clap(long)]
    rpc_port: u16,

    /// Network port.
    #[clap(long)]
    p2p_port: u16,
}

#[tokio::main]
async fn main() {
    let flags = Flags::parse();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
        .try_init()
        .expect("to succeed");

    // start overseer
    let (overseer_tx, mut overseer_rx) = mpsc::channel(64);

    // start p2p
    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let socket = TcpListener::bind(format!("127.0.0.1:{}", flags.p2p_port))
        .await
        .unwrap();

    let p2p_tx = overseer_tx.clone();
    tokio::spawn(async move { p2p::P2p::new(socket, cmd_rx, p2p_tx).run().await });
    tokio::spawn(async move {
        rpc::run_server(overseer_tx, String::from("127.0.0.1"), flags.rpc_port).await
    });

    // start chainstate
    let mut chainstate = chainstate::Chainstate::new();

    loop {
        match overseer_rx.recv().await.expect("channel to stay open") {
            OverseerEvent::Message(message) => match message {
                Message::Transaction(transaction) => {
                    tracing::error!(
                        target: LOG_TARGET,
                        tx = ?transaction,
                        "received transaction from p2p"
                    );

                    if let Err(err) = chainstate.import_transaction(transaction) {
                        tracing::error!(
                            target: LOG_TARGET,
                            err = ?err,
                            "failed to import transaction"
                        );
                    }
                }
                Message::Block(block) => {
                    tracing::error!(
                        target: LOG_TARGET,
                        block = ?block,
                        "received block from p2p"
                    );

                    if let Err(err) = chainstate.import_block(block) {
                        tracing::error!(
                            target: LOG_TARGET,
                            err = ?err,
                            "failed to import block"
                        );
                    }
                }
            },
            OverseerEvent::ConnectToPeer(address, port) => {
                tracing::debug!(
                    target: LOG_TARGET,
                    address = address,
                    port = port,
                    "attempt to connect to remote peer",
                );

                cmd_tx
                    .send(Command::ConnectToPeer(address, port))
                    .await
                    .expect("channel to stay open");
            }
        }
    }
}
