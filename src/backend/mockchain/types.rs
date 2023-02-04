use serde::{Deserialize, Serialize};

use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use tokio::sync::oneshot;

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

impl Distribution<Message> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Message {
        match rng.gen_range(0..=4) {
            0 => Message::Transaction(Transaction::new(
                rng.gen::<AccountId>(),
                rng.gen::<AccountId>(),
                rng.gen::<u64>(),
            )),
            1 => Message::Block(Block::from_transactions(
                (0..rng.gen_range(1..=5))
                    .map(|_| {
                        Transaction::new(
                            rng.gen::<AccountId>(),
                            rng.gen::<AccountId>(),
                            rng.gen::<u64>(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )),
            2 => Message::Vote(Vote::new(
                rng.gen::<BlockId>(),
                rng.gen::<PeerId>(),
                rng.gen::<bool>(),
            )),
            3 => Message::Dispute(Dispute::new(rng.gen::<BlockId>(), rng.gen::<PeerId>())),
            4 => Message::PeerExchange(Pex::new(
                (0..rng.gen_range(2..=6))
                    .map(|_| {
                        (
                            format!(
                                "{}.{}.{}.{}",
                                rng.gen_range(1..=255),
                                rng.gen_range(0..=255),
                                rng.gen_range(0..=255),
                                rng.gen_range(0..=255),
                            ),
                            rng.gen::<u16>(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )),
            _ => todo!(),
        }
    }
}
