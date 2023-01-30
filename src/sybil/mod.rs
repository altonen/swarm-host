/*
use crate::types::PeerId;

use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
};

use std::{error::Error, pin::Pin, time::Duration};

const LOG_TARGET: &'static str = "sybil";

pub mod interface;
pub mod types;

// pub use interface::Interface;

// TODO: does peer really need a task?

/// TODO: documentation
pub struct Node {
    rx: Receiver<types::Event>,
    rx_msg: Receiver<(String, String)>,
    tx: Sender<types::Event>,
    id: PeerId,
    timeout: Duration,
}

impl Node {
    /// Create a new Sybil node.
    pub fn new(
        rx: Receiver<types::Event>,
        rx_msg: Receiver<(String, String)>,
        tx: Sender<types::Event>,
        timeout: Duration,
    ) -> Self {
        let id = rand::thread_rng().gen::<PeerId>();

        tracing::info!(target: "sybil", id = id, "starting sybil node");

        Self {
            rx,
            rx_msg,
            tx,
            timeout,
            id,
        }
    }

    /// Start event loop for the sybil node.
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                event = self.rx.recv() => match event {
                    Some(event) => match event {
                        _ => todo!(),
                    }
                    None => break,
                },
                result = self.rx_msg.recv() => match result {
                    Some((protocol, message)) => {
                        tracing::trace!(target: LOG_TARGET, protocol = protocol, "sybil node received a message");
                    }
                    None => panic!("expect channel to stay open"),
                },
                _ = tokio::time::sleep(self.timeout) => {
                    self.tx.send(types::Event::SybilMessage {
                        peer: self.id,
                        protocol: String::from("/proto/1.0.1"),
                        message: format!("hello from node {}", self.id),
                    })
                    .await
                    .expect("channel to stay to open");
                }
            }
        }
    }
}
*/
