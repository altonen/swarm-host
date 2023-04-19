use crate::backend::NetworkBackend;

use std::collections::{HashMap, HashSet};

// TODO: store total sent/received
// TODO: store total bandwidth sent/received bandwidth consumed
// TODO: store total redudant bandwidth consumed

/// Peer heuristics
pub struct PeerHeuristics<T: NetworkBackend> {
    /// Total sent notifications.
    total_sent: HashMap<T::Protocol, usize>,

    /// Total received
    total_received: HashMap<T::Protocol, usize>,

    /// Connections to interfaces.
    connections: HashSet<T::InterfaceId>,
}

impl<T: NetworkBackend> PeerHeuristics<T> {
    /// Create new [`PeerHeuristics`].
    pub fn new() -> Self {
        Self {
            total_sent: HashMap::new(),
            total_received: HashMap::new(),
            connections: HashSet::new(),
        }
    }

    /// Register that a message was received from `peer`.
    pub fn register_message_received(&mut self, protocol: &T::Protocol, hash: u64, size: usize) {
        // TODO: do something with the hash?
        *self.total_received.entry(protocol.to_owned()).or_default() += size;
    }

    /// Register that a message was sent to `peer`.
    pub fn register_message_sent(&mut self, protocol: &T::Protocol, hash: u64, size: usize) {
        // TODO: do something with the hash?
        *self.total_sent.entry(protocol.to_owned()).or_default() += size;
    }

    /// Convert peer heuristics into JSON so it can be displayed.
    pub fn into_json(&self) -> String {
        todo!("convert the data into json");
    }
}
