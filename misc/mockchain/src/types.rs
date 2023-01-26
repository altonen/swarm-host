use serde::{Deserialize, Serialize};

use std::net::IpAddr;

/// Unique account ID.
pub type AccountId = u64;

/// Unique block ID.
pub type BlockId = u64;

/// Unique message ID.
pub type MessageId = u64;

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

    /// Publish generic message on the network
    PublishMessage(Message),
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct Vote {
    block: BlockId,
    peer: PeerId,
    vote: bool, // true == yay, false == nay,
}

impl Vote {
    pub fn new(block: BlockId, peer: PeerId, vote: bool) -> Self {
        Self { block, peer, vote }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct Dispute {
    block: BlockId,
    peer: PeerId,
}

impl Dispute {
    pub fn new(block: BlockId, peer: PeerId) -> Self {
        Self { block, peer }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Pex {
    peers: Vec<(String, u16)>,
}

impl Pex {
    pub fn new(peers: Vec<(String, u16)>) -> Self {
        Self { peers }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum Message {
    Transaction(Transaction),
    Block(Block),
    Vote(Vote),
    Dispute(Dispute),
    PeerExchange(Pex),
}

#[derive(Debug, PartialEq, Eq)]
pub enum OverseerEvent {
    Message(Subsystem, Message),
    ConnectToPeer(String, u16),
    DisconnectPeer(PeerId),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Subsystem {
    Gossip,
    P2p,
    Rpc,
}
