use crate::{
    backend::{NetworkBackend, WithMessageInfo},
    heuristics::peer::PeerHeuristics,
};

use tokio::sync::mpsc;

use std::collections::{HashMap, HashSet};

mod peer;

// TODO: have protocol-level heuristics (from all peers)
// TODO: have peer-level heuristics (for every protocol)

/// Logging target for the file.
const LOG_TARGET: &'static str = "heuristics";

/// Logging target for binary messages.
const LOG_TARGET_MSG: &'static str = "heuristics::msg";

/// Interface heuristics.
struct InterfaceHeuristics<T: NetworkBackend> {
    /// Heuristics for each peer connected to the interface.
    peers: HashMap<T::PeerId, PeerHeuristics<T>>,

    /// Interfaces the interface is connected to.
    links: HashSet<T::InterfaceId>,
}

impl<T: NetworkBackend> InterfaceHeuristics<T> {
    /// Create new [`InterfaceHeuristics`].
    fn new() -> Self {
        Self {
            peers: Default::default(),
            links: Default::default(),
        }
    }
}

/// Events sent by the [`HeuristicsHandle`] to [`HeuristicsBackend`].
#[derive(Debug)]
enum HeuristicsEvent<T: NetworkBackend> {
    /// Register interface.
    RegisterInterface {
        /// Interface ID.
        interface: T::InterfaceId,
    },

    /// Link interfaces.
    LinkInterfaces {
        /// ID of the first interface.
        first: T::InterfaceId,

        /// ID of the second interface.
        second: T::InterfaceId,
    },

    /// Unlink interfaces.
    UnlinkInterfaces {
        /// ID of the first interface.
        first: T::InterfaceId,

        /// ID of the second interface.
        second: T::InterfaceId,
    },

    /// Register peer.
    RegisterPeer {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Peer ID.
        peer: T::PeerId,
    },

    /// Unregister interface
    UnregisterPeer {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Peer ID.
        peer: T::PeerId,
    },

    /// Message was received from `peer` to `interface`.
    MessageReceived {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Protocol.
        protocol: T::Protocol,

        /// Peer ID.
        peer: T::PeerId,

        /// Notification hash.
        hash: u64,

        /// Notification size.
        size: usize,
    },

    /// Message was sent from `interface` to `peers`.
    MessageSent {
        /// Interface ID.
        interface: T::InterfaceId,

        /// Protocol.
        protocol: T::Protocol,

        /// Peer IDs.
        peers: Vec<T::PeerId>,

        /// Notification hash.
        hash: u64,

        /// Notification size.
        size: usize,
    },
}

/// Handle to registers events to [`HeuristicsBackend`].
#[derive(Debug, Clone)]
pub struct HeuristicsHandle<T: NetworkBackend> {
    /// TX channel for sending events to [`HeuristicsBackend`].
    tx: mpsc::UnboundedSender<HeuristicsEvent<T>>,
}

impl<T: NetworkBackend> HeuristicsHandle<T> {
    /// Register interface.
    pub fn register_interface(&self, interface: T::InterfaceId) {
        self.tx
            .send(HeuristicsEvent::RegisterInterface { interface })
            .expect("channel to stay open");
    }

    /// Link interface.
    pub fn link_interfaces(&self, first: T::InterfaceId, second: T::InterfaceId) {
        self.tx
            .send(HeuristicsEvent::LinkInterfaces { first, second })
            .expect("channel to stay open");
    }

    /// Unlink interface.
    pub fn unlink_interfaces(&self, first: T::InterfaceId, second: T::InterfaceId) {
        self.tx
            .send(HeuristicsEvent::UnlinkInterfaces { first, second })
            .expect("channel to stay open");
    }

    /// Register `peer` to `interface`'s known peers.
    pub fn register_peer(&self, interface: T::InterfaceId, peer: T::PeerId) {
        self.tx
            .send(HeuristicsEvent::RegisterPeer { interface, peer })
            .expect("channel to stay open");
    }

    /// Unregister `peer` from `interface`'s known peers.
    pub fn unregister_peer(&self, interface: T::InterfaceId, peer: T::PeerId) {
        self.tx
            .send(HeuristicsEvent::UnregisterPeer { interface, peer })
            .expect("channel to stay open");
    }

    /// Register that `notification` was received from `peer` to `interface`.
    pub fn register_notification_received(
        &self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        notification: &T::Message,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageReceived {
                interface,
                protocol,
                peer,
                hash: notification.hash(),
                size: notification.size(),
            })
            .expect("channel to stay open");
    }

    /// Register that `notification` was forwarded from `interface` to `peers`.
    pub fn register_notification_sent(
        &self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        peers: Vec<T::PeerId>,
        notification: &T::Message,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageSent {
                interface,
                protocol,
                peers,
                hash: notification.hash(),
                size: notification.size(),
            })
            .expect("channel to stay open");
    }

    /// Register that `request` was received from `peer` to `interface.`
    pub fn register_request_received(
        &self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        request: &T::Request,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageReceived {
                interface,
                protocol,
                peer,
                hash: request.hash(),
                size: request.size(),
            })
            .expect("channel to stay open");
    }

    /// Register that `request` was sent to `peer` from `interface.`
    pub fn register_request_sent(
        &self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        request: &T::Request,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageSent {
                interface,
                protocol,
                peers: vec![peer],
                hash: request.hash(),
                size: request.size(),
            })
            .expect("channel to stay open");
        todo!();
    }

    /// Register that `response` was received from `peer` to `interface.`
    pub fn register_response_received(
        &self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        response: &T::Response,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageReceived {
                interface,
                protocol,
                peer,
                hash: response.hash(),
                size: response.size(),
            })
            .expect("channel to stay open");
    }

    /// Register that `response` was sent to `peer` from `interface.`
    pub fn register_response_sent(
        &self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        peer: T::PeerId,
        response: &T::Response,
    ) {
        self.tx
            .send(HeuristicsEvent::MessageSent {
                interface,
                protocol,
                peers: vec![peer],
                hash: response.hash(),
                size: response.size(),
            })
            .expect("channel to stay open");
    }
}

/// Heuristic backend.
pub struct HeuristicsBackend<T: NetworkBackend> {
    /// RX channel for receiving events from [`HeuristicsHandle`].
    rx: mpsc::UnboundedReceiver<HeuristicsEvent<T>>,

    /// Active interfaces.
    interfaces: HashMap<T::InterfaceId, InterfaceHeuristics<T>>,
}

impl<T: NetworkBackend> HeuristicsBackend<T> {
    /// Create new [`HeuristicsBackend`].
    pub fn new() -> (Self, HeuristicsHandle<T>) {
        let (tx, rx) = mpsc::unbounded_channel();

        (
            Self {
                rx,
                interfaces: Default::default(),
            },
            HeuristicsHandle { tx },
        )
    }

    /// Run the event loop of [`HeuristicsBackend`].
    async fn run(mut self) {
        while let Some(event) = self.rx.recv().await {
            match event {
                HeuristicsEvent::RegisterInterface { interface } => {
                    self.interfaces
                        .insert(interface, InterfaceHeuristics::new());
                }
                HeuristicsEvent::LinkInterfaces { first, second } => {
                    if !self.interfaces.contains_key(&first)
                        || !self.interfaces.contains_key(&second)
                    {
                        continue;
                    }

                    self.interfaces
                        .get_mut(&first)
                        .expect("interface to exist")
                        .links
                        .insert(second);
                    self.interfaces
                        .get_mut(&second)
                        .expect("interface to exist")
                        .links
                        .insert(first);
                }
                HeuristicsEvent::UnlinkInterfaces { first, second } => {
                    if !self.interfaces.contains_key(&first)
                        || !self.interfaces.contains_key(&second)
                    {
                        continue;
                    }

                    self.interfaces
                        .get_mut(&first)
                        .expect("interface to exist")
                        .links
                        .remove(&second);
                    self.interfaces
                        .get_mut(&second)
                        .expect("interface to exist")
                        .links
                        .remove(&first);
                }
                HeuristicsEvent::RegisterPeer { interface, peer } => {
                    let Some(interface) = self.interfaces.get_mut(&interface) else {
                        continue;
                    };

                    interface.peers.insert(peer, PeerHeuristics::new());
                }
                HeuristicsEvent::UnregisterPeer { interface, peer } => {
                    let Some(interface) = self.interfaces.get_mut(&interface) else {
                        continue;
                    };

                    interface.peers.remove(&peer);
                }
                HeuristicsEvent::MessageReceived {
                    interface,
                    protocol,
                    peer,
                    hash,
                    size,
                } => {
                    let Some(interface) = self.interfaces.get_mut(&interface) else {
                        continue;
                    };

                    if let Some(peer) = interface.peers.get_mut(&peer) {
                        peer.register_message_received(&protocol, hash, size)
                    }
                }
                HeuristicsEvent::MessageSent {
                    interface,
                    protocol,
                    peers,
                    hash,
                    size,
                } => {
                    let Some(interface) = self.interfaces.get_mut(&interface) else {
                        continue;
                    };

                    for peer in peers {
                        if let Some(peer) = interface.peers.get_mut(&peer) {
                            peer.register_message_sent(&protocol, hash, size)
                        }
                    }
                }
            }
        }
    }
}
