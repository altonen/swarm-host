use crate::{
    backend::{mockchain::MockchainBackend, Idable, NetworkBackend, WithMessageInfo},
    executor::{FromExecutorObject, IntoExecutorObject},
};

use parity_scale_codec::{Decode, Encode};
use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyString},
};
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use serde::{Deserialize, Serialize};

use std::{collections::hash_map::DefaultHasher, hash::Hasher};

/// Unique ID identifying the interface.
pub type InterfaceId = usize;

/// Unique ID identifying the peer.
pub type PeerId = u64;

/// Unique account ID.
pub type AccountId = u64;

/// Unique block ID.
pub type BlockId = u64;

/// Unique request ID.
#[derive(Debug, Copy, Clone, FromPyObject, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);

impl IntoPy<PyObject> for RequestId {
    fn into_py(self, py: Python) -> PyObject {
        self.0.into_py(py)
    }
}

// TODO: type conversions should be kept in one place
impl IntoExecutorObject for <MockchainBackend as NetworkBackend>::PeerId {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        self.into_py(context)
    }
}

impl FromExecutorObject for <MockchainBackend as NetworkBackend>::PeerId {
    type ExecutorType<'a> = &'a PyAny;

    fn from_executor_object(executor_type: &'_ Self::ExecutorType<'_>) -> Self {
        executor_type.extract().unwrap()
    }
}

impl FromExecutorObject for <MockchainBackend as NetworkBackend>::RequestId {
    type ExecutorType<'a> = &'a PyAny;

    fn from_executor_object(executor_type: &'_ Self::ExecutorType<'_>) -> Self {
        executor_type.extract().unwrap()
    }
}

impl FromExecutorObject for <MockchainBackend as NetworkBackend>::Request {
    type ExecutorType<'a> = &'a PyAny;

    // TODO: this is bad, use `extract()` instead
    fn from_executor_object(executor_type: &'_ Self::ExecutorType<'_>) -> Self {
        let dict = executor_type.downcast::<pyo3::types::PyDict>().unwrap();
        let id = dict.get_item("id").unwrap().extract::<RequestId>().unwrap();
        let payload = dict
            .get_item("payload")
            .unwrap()
            .extract::<Vec<u8>>()
            .unwrap();

        Self { id, payload }
    }
}

impl IntoExecutorObject for <MockchainBackend as NetworkBackend>::Message {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        self.into_py(context)
    }
}

impl FromExecutorObject for <MockchainBackend as NetworkBackend>::Message {
    type ExecutorType<'a> = &'a PyAny;

    fn from_executor_object(_executor_type: &'_ Self::ExecutorType<'_>) -> Self {
        todo!();
    }
}

/// Transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, FromPyObject, Encode, Decode)]
pub struct Transaction {
    sender: AccountId,
    receiver: AccountId,
    amount: u64,
}

impl IntoPy<PyObject> for Transaction {
    fn into_py(self, py: Python) -> PyObject {
        let tx_dict = PyDict::new(py);
        tx_dict.set_item("sender", self.sender).unwrap();
        tx_dict.set_item("receiver", self.receiver).unwrap();
        tx_dict.set_item("amount", self.amount).unwrap();
        tx_dict.into()
    }
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

impl Distribution<Transaction> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Transaction {
        Transaction::new(
            rng.gen::<AccountId>(),
            rng.gen::<AccountId>(),
            rng.gen::<u64>(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, FromPyObject, Encode, Decode)]
pub struct Block {
    number: u128,
    time: u64,
    transactions: Vec<Transaction>,
}

impl IntoPy<PyObject> for Block {
    fn into_py(self, py: Python) -> PyObject {
        let blk_dict = PyDict::new(py);
        blk_dict.set_item("time", self.time).unwrap();

        let tx_list = PyList::empty(py);
        for tx in self.transactions.into_iter() {
            let tx_dict = PyDict::new(py);
            tx_dict.set_item("payload", tx.into_py(py)).unwrap();
            tx_list.append(tx_dict).unwrap();
        }

        blk_dict.set_item("transactions", tx_list).unwrap();
        blk_dict.into()
    }
}

impl Block {
    /// Create new empty block.
    pub fn new(number: u128) -> Self {
        Self::from_transactions(number, Vec::new())
    }

    /// Create new block from transactions.
    pub fn from_transactions(number: u128, transactions: impl Into<Vec<Transaction>>) -> Self {
        Self {
            number,
            transactions: transactions.into(),
            // TODO: unix timestamp
            time: 1337u64,
        }
    }
}

#[derive(
    Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, FromPyObject, Encode, Decode,
)]
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

impl IntoPy<PyObject> for Vote {
    fn into_py(self, py: Python) -> PyObject {
        let vote_dict = PyDict::new(py);
        vote_dict.set_item("block", self.block).unwrap();
        vote_dict.set_item("peer", self.peer).unwrap();
        vote_dict.set_item("vote", self.vote).unwrap();
        vote_dict.into()
    }
}

#[derive(
    Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, FromPyObject, Encode, Decode,
)]
pub struct Dispute {
    block: BlockId,
    peer: PeerId,
}

impl Dispute {
    pub fn new(block: BlockId, peer: PeerId) -> Self {
        Self { block, peer }
    }
}

impl IntoPy<PyObject> for Dispute {
    fn into_py(self, py: Python) -> PyObject {
        let dispute_dict = PyDict::new(py);
        dispute_dict.set_item("block", self.block).unwrap();
        dispute_dict.set_item("peer", self.peer).unwrap();
        dispute_dict.into()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, FromPyObject, Encode, Decode)]
pub struct Pex {
    peers: Vec<(String, u16)>,
}

impl Pex {
    pub fn new(peers: Vec<(String, u16)>) -> Self {
        Self { peers }
    }
}

impl IntoPy<PyObject> for Pex {
    fn into_py(self, py: Python) -> PyObject {
        let pex_dict = PyDict::new(py);

        let pex_list = PyList::empty(py);
        for pair in &self.peers {
            // TODO: use tuple instead of dict
            let entry = PyDict::new(py);
            entry.set_item("Entry", pair).unwrap();
            pex_list.append(entry).unwrap();
        }

        pex_dict.set_item("peers", pex_list).unwrap();
        pex_dict.into()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Encode, Decode)]
pub enum Message {
    Transaction(Transaction),
    Block(Block),
    Vote(Vote),
    Dispute(Dispute),
    PeerExchange(Pex),
}

impl IntoPy<PyObject> for Message {
    fn into_py(self, py: Python<'_>) -> PyObject {
        match self {
            Message::Transaction(transaction) => {
                let dict = PyDict::new(py);
                dict.set_item("Transaction", transaction.into_py(py))
                    .unwrap();
                dict.into()
            }
            Message::Block(block) => {
                let dict = PyDict::new(py);
                dict.set_item("Block", block.into_py(py)).unwrap();
                dict.into()
            }
            Message::Vote(vote) => {
                let dict = PyDict::new(py);
                dict.set_item("Vote", vote.into_py(py)).unwrap();
                dict.into()
            }
            Message::Dispute(dispute) => {
                let dict = PyDict::new(py);
                dict.set_item("Dispute", dispute.into_py(py)).unwrap();
                dict.into()
            }
            Message::PeerExchange(pex) => {
                let dict = PyDict::new(py);
                dict.set_item("PeerExchange", pex.into_py(py)).unwrap();
                dict.into()
            }
        }
    }
}

impl<'a> FromPyObject<'a> for Message {
    fn extract(_object: &'a PyAny) -> PyResult<Self> {
        todo!();
    }
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
                rng.gen(),
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

impl WithMessageInfo for Message {
    fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.encode());
        hasher.finish()
    }

    fn size(&self) -> usize {
        self.encode().len()
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

impl FromExecutorObject for <MockchainBackend as NetworkBackend>::Protocol {
    type ExecutorType<'a> = &'a PyAny;

    fn from_executor_object(executor_type: &'_ Self::ExecutorType<'_>) -> Self {
        let protocol = executor_type
            .downcast::<PyString>()
            .unwrap()
            .to_str()
            .unwrap();

        match protocol {
            "Transaction" => ProtocolId::Transaction,
            "Block" => ProtocolId::Block,
            "PeerExchange" => ProtocolId::PeerExchange,
            "BlockRequest" => ProtocolId::BlockRequest,
            "Generic" => ProtocolId::Generic,
            _ => panic!("invalid protocol received"),
        }
    }
}

impl IntoExecutorObject for <MockchainBackend as NetworkBackend>::Protocol {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        match self {
            ProtocolId::Transaction => PyString::new(context, "Transaction").into(),
            ProtocolId::Block => PyString::new(context, "Block").into(),
            ProtocolId::PeerExchange => PyString::new(context, "PeerExchange").into(),
            ProtocolId::BlockRequest => PyString::new(context, "BlockRequest").into(),
            ProtocolId::Generic => PyString::new(context, "Generic").into(),
        }
    }
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
    /// message to remote node before doing anything elsE.
    _Outbound,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, FromPyObject)]
pub struct Request {
    /// Unique ID of the request.
    id: RequestId,

    /// Payload of the request.
    payload: Vec<u8>,
}

impl Request {
    #[allow(dead_code)]
    pub fn new(id: RequestId, payload: Vec<u8>) -> Self {
        Self { id, payload }
    }
}

impl IntoExecutorObject for <MockchainBackend as NetworkBackend>::Request {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        let fields = PyDict::new(context);
        fields.set_item("id", self.id.into_py(context)).unwrap();
        fields
            .set_item("payload", self.payload.into_py(context))
            .unwrap();

        let request = PyDict::new(context);
        request.set_item("Request", fields).unwrap();
        request.into()
    }
}

impl Idable<MockchainBackend> for Request {
    fn id(&self) -> &<MockchainBackend as NetworkBackend>::RequestId {
        &self.id
    }
}

impl WithMessageInfo for Request {
    fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.payload);
        hasher.finish()
    }

    fn size(&self) -> usize {
        self.payload.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode)]
pub struct BlockRequest {
    /// Start from this block.
    start_from: u128,

    /// Number of blocks to request.
    num_blocks: u8,
}

impl BlockRequest {
    #[allow(dead_code)]
    pub fn new(start_from: u128, num_blocks: u8) -> Self {
        Self {
            start_from,
            num_blocks,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode)]
pub struct BlockResponse {
    /// Blocks
    blocks: Vec<Block>,
}

impl BlockResponse {
    #[allow(dead_code)]
    pub fn new(blocks: Vec<Block>) -> Self {
        Self { blocks }
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
    #[allow(dead_code)]
    pub fn new(id: RequestId, payload: Vec<u8>) -> Self {
        Self { id, payload }
    }

    pub fn _id(&self) -> &RequestId {
        &self.id
    }

    pub fn _payload(&self) -> &Vec<u8> {
        &self.payload
    }
}

impl IntoExecutorObject for <MockchainBackend as NetworkBackend>::Response {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        let fields = PyDict::new(context);
        fields.set_item("id", self.id.into_py(context)).unwrap();
        fields
            .set_item("payload", self.payload.into_py(context))
            .unwrap();

        let request = PyDict::new(context);
        request.set_item("Response", fields).unwrap();
        request.into()
    }
}

impl Idable<MockchainBackend> for Response {
    fn id(&self) -> &<MockchainBackend as NetworkBackend>::RequestId {
        &self.id
    }
}

impl WithMessageInfo for Response {
    fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.payload);
        hasher.finish()
    }

    fn size(&self) -> usize {
        self.payload.len()
    }
}
