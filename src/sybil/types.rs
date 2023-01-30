/*
use crate::types::PeerId;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

/// TODO: documentation
#[derive(Debug)]
pub enum Event {
    /// Peer connected.
    Connected {
        /// Peer ID.
        peer: PeerId,

        /// Supported protocols.
        protocols: Vec<String>,

        /// TX channel for communication with the peer.
        tx: Sender<(String, String)>,
    },

    /// Peer disconnected.
    Disconnected(PeerId),

    /// Message received from peer to a Sybil interface.
    Message {
        /// Peer ID.
        peer: PeerId,

        /// Protocol of the message.
        protocol: String,

        /// Received message.
        message: String,
    },

    /// Message received from one of the Sybil nodes.
    SybilMessage {
        /// Peer ID.
        peer: PeerId,

        /// Protocol of the message.
        protocol: String,

        /// Received message.
        message: String,
    },
}

/// TODO: documentation
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    /// Handshake message containing peer information.
    Handshake {
        /// Peer ID.
        peer: PeerId,

        /// Supported protocols
        protocols: Vec<String>,
    },
}
*/
