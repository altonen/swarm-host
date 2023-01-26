use crate::types::{
    AccountId, Block, BlockId, Dispute, Message, OverseerEvent, PeerId, Pex, Subsystem,
    Transaction, Vote,
};

use rand::{
    distributions::{Distribution, Standard},
    Rng, SeedableRng,
};
use tokio::sync::mpsc::Sender;

const LOG_TARGET: &'static str = "gossip";

pub struct GossipEngine {
    overseer_tx: Sender<OverseerEvent>,
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

// TODO: pass seed from overseer
impl GossipEngine {
    pub fn new(overseer_tx: Sender<OverseerEvent>) -> Self {
        Self { overseer_tx }
    }

    pub async fn run(mut self) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1337u64);

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(rng.gen_range(0..5))).await;
            let message: Message = rand::random();

            tracing::trace!(
                target: LOG_TARGET,
                message = ?message,
                "publish message on the network"
            );

            self.overseer_tx
                .send(OverseerEvent::Message(Subsystem::Gossip, message))
                .await
                .expect("channel to stay open");
        }
    }
}
