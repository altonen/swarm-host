use serde::{Deserialize, Serialize};

/// Unique account ID.
pub type AccountId = u64;

/// Transaction.
#[derive(Debug, Serialize, Deserialize)]
struct Transaction {
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

#[derive(Debug, Serialize, Deserialize)]
struct Block {
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
            time: 1337u64,
        }
    }
}
