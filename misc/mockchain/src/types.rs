use serde::{Deserialize, Serialize};

use std::net::IpAddr;

/// Unique account ID.
pub type AccountId = u64;

/// Transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    sender: AccountId,
    receiver: AccountId,
    amount: u64,
}

impl Transaction {
    /// Create new transaction.
    pub fn new(sender: AccountId, receiver: AccountId, amount: u64) -> Self {
        Self {
            sender,
            receiver,
            amount,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    time: u64,
    transactions: Vec<Transaction>,
}

impl Block {
    /// Create new empty block.
    pub fn new() -> Self {
        Self::from_transactions(Vec::new())
    }

    /// Create new block from transactions.
    pub fn from_transactions(transactions: impl Into<Vec<Transaction>>) -> Self {
        Self {
            transactions: transactions.into(),
            // TODO: unix timestamp
            time: 1337u64,
        }
    }
}

/// Unique peer ID.
pub type PeerId = u64;

/// Commands send to P2P.
#[derive(Debug)]
pub enum Command {
    /// Publish transaction on the network.
    PublishTransaction(Transaction),

    /// Publish block on the network.
    PublishBlock(Block),

    /// Disconnect peer.
    DisconnectPeer(PeerId),

    /// Attempt to establish connection with a peer.
    // TODO: `oneshot::Sender` for result?
    ConnectToPeer(String, u16),
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Message {
    Transaction(Transaction),
    Block(Block),
}

#[derive(Debug, PartialEq, Eq)]
pub enum OverseerEvent {
    Message(Message),
    ConnectToPeer(String, u16),
}
