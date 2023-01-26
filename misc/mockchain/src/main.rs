use crate::types::{Command, Message, OverseerEvent};

use clap::Parser;
use tokio::{net::TcpListener, sync::mpsc};
use tracing_subscriber::{fmt::format::FmtSpan, prelude::*};

const LOG_TARGET: &'static str = "overseer";

// TODO: mitä kaikkea täytyy olla implmentoituna:
//   - bind to interface
//   - publish block
//   - publish transaction
//   - connect to peer
//   - disconnect peer
//
//   - implement gossip engine which sends random messages
//
//   - create genesis block and seed some accounts
//   - rudimentary syncing
//   - rudimentary block production
//   - submit transaction
//   - query if tx is present
//   - query if block is present
//
// TODO: bind to interface instead

mod chainstate;
mod gossip;
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

    let gossip_tx = overseer_tx.clone();
    tokio::spawn(async move { gossip::GossipEngine::new(gossip_tx).run().await });

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
                _ => panic!("unexpected message type"),
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
            OverseerEvent::DisconnectPeer(peer) => {
                tracing::debug!(target: LOG_TARGET, id = peer, "disconnect peer",);

                cmd_tx
                    .send(Command::DisconnectPeer(peer))
                    .await
                    .expect("channel to stay open");
            }
        }
    }
}
