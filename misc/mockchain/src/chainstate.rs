use crate::types::{Block, Transaction};

use std::{collections::HashMap, time::Instant};

pub struct Chainstate {
    block_index: HashMap<u64, Block>,
    tx_index: HashMap<u64, Transaction>,
    txpool: Vec<Transaction>,
    last_block: Instant,
}

impl Chainstate {
    pub fn new() -> Self {
        Self {
            block_index: HashMap::new(),
            tx_index: HashMap::new(),
            txpool: Vec::new(),
            last_block: Instant::now(),
        }
    }

    pub fn import_block(&mut self, block: Block) -> Result<(), ()> {
        todo!();
    }

    pub fn import_transaction(&mut self, transaction: Transaction) -> Result<(), ()> {
        todo!();
    }

    pub fn produce_block(&mut self) -> Block {
        todo!();
    }
}
