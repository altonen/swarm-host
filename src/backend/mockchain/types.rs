use crate::backend::{mockchain::MockchainBackend, IdableRequest, NetworkBackend};

use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use std::net::IpAddr;

/// Unique ID identifying the interface.
pub type InterfaceId = usize;

/// Unique ID identifying the peer.
pub type PeerId = u64;

/// Unique account ID.
pub type AccountId = u64;

/// Unique block ID.
pub type BlockId = u64;

/// Unique message ID.
pub type MessageId = u64;

/// Unique request ID.
pub type RequestId = u64;

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

    /// Get sender.
    pub fn sender(&self) -> AccountId {
        self.sender
    }

    /// Get receiver.
    pub fn receiver(&self) -> AccountId {
        self.receiver
    }

    pub fn amount(&self) -> u64 {
        self.amount
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

/// Supported protocols.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolId {
    /// Transaction protocol.
    Transaction,

    /// Block protocol.
    Block,

    /// Peer exchange protocol.
    PeerExchange,

    /// Block request
    BlockRequest,

    /// Generic protocol.
    Generic,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Handshake {
    /// Unique ID of the peer.
    pub peer: PeerId,

    /// Supported protocols of the peer.
    pub protocols: Vec<ProtocolId>,
}

#[derive(Debug)]
pub enum ConnectionType {
    /// Local node received a connection and is expecting to
    /// read a handshake from the socket as first message.
    Inbound,

    /// Local node initiated the connection and must send a handshake
    /// message to remote node before doing anything else.
    Outbound,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Request {
    /// Unique ID of the request.
    id: RequestId,

    /// Payload of the request.
    payload: Vec<u8>,
}

impl Request {
    pub fn new(id: RequestId, payload: Vec<u8>) -> Self {
        Self { id, payload }
    }
}

impl IdableRequest<MockchainBackend> for Request {
    fn id(&self) -> &<MockchainBackend as NetworkBackend>::RequestId {
        &self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Response {
    /// ID of the request.
    id: RequestId,

    /// Payload of the response.
    payload: Vec<u8>,
}

impl Response {
    pub fn new(id: RequestId, payload: Vec<u8>) -> Self {
        Self { id, payload }
    }

    pub fn id(&self) -> &RequestId {
        &self.id
    }

    pub fn payload(&self) -> &Vec<u8> {
        &self.payload
    }
}
