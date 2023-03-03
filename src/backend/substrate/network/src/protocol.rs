// This file is part of Substrate.

// Copyright (C) 2017-2022 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::config;

use bytes::Bytes;
use codec::Encode;
use libp2p::{
    core::connection::ConnectionId,
    swarm::{
        behaviour::FromSwarm, ConnectionHandler, IntoConnectionHandler, NetworkBehaviour,
        NetworkBehaviourAction, PollParameters,
    },
    Multiaddr, PeerId,
};
use log::warn;
use notifications::{Notifications, NotificationsOut};
use sc_network_common::{
    config::NonReservedPeerMode,
    error,
    protocol::{role::Roles, ProtocolName},
};
use std::{
    collections::{HashSet, VecDeque},
    iter,
    task::Poll,
};

mod notifications;

pub mod message;

pub use notifications::{NotificationsSink, NotifsHandlerError, Ready};

/// Maximum size used for notifications in the block announce and transaction protocols.
// Must be equal to `max(MAX_BLOCK_ANNOUNCE_SIZE, MAX_TRANSACTIONS_SIZE)`.
pub(crate) const BLOCK_ANNOUNCES_TRANSACTIONS_SUBSTREAM_SIZE: u64 = 16 * 1024 * 1024;

/// Number of hardcoded peersets (the constants right above). Any set whose identifier is equal or
/// superior to this value corresponds to a user-defined protocol.
const NUM_HARDCODED_PEERSETS: usize = 1;

/// Harcoded peerset id for syncing.
const _HARDCODED_PEERSETS_SYNC: sc_peerset::SetId = sc_peerset::SetId::from(0);

// Lock must always be taken in order declared here.
pub struct Protocol {
    /// Pending list of messages to return from `poll` as a priority.
    pending_messages: VecDeque<CustomMessageOutcome>,
    /// Used to report reputation changes.
    peerset_handle: sc_peerset::PeersetHandle,
    /// Handles opening the unique substream and sending and receiving raw messages.
    behaviour: Notifications,
    /// List of notifications protocols that have been registered.
    notification_protocols: Vec<ProtocolName>,
}

impl Protocol {
    /// Create a new instance.
    pub fn new(
        roles: Roles,
        network_config: &config::NetworkConfiguration,
        block_announces_protocol: sc_network_common::config::NonDefaultSetConfig,
    ) -> error::Result<(Self, sc_peerset::PeersetHandle, Vec<(PeerId, Multiaddr)>)> {
        let mut known_addresses = Vec::new();

        let (peerset, peerset_handle) = {
            let mut sets =
                Vec::with_capacity(NUM_HARDCODED_PEERSETS + network_config.extra_sets.len());

            let mut default_sets_reserved = HashSet::new();
            for reserved in network_config.default_peers_set.reserved_nodes.iter() {
                default_sets_reserved.insert(reserved.peer_id);

                if !reserved.multiaddr.is_empty() {
                    known_addresses.push((reserved.peer_id, reserved.multiaddr.clone()));
                }
            }

            let mut bootnodes = Vec::with_capacity(network_config.boot_nodes.len());
            for bootnode in network_config.boot_nodes.iter() {
                bootnodes.push(bootnode.peer_id);
            }

            // Set number 0 is used for block announces.
            sets.push(sc_peerset::SetConfig {
                in_peers: network_config.default_peers_set.in_peers,
                out_peers: network_config.default_peers_set.out_peers,
                bootnodes,
                reserved_nodes: default_sets_reserved.clone(),
                reserved_only: network_config.default_peers_set.non_reserved_mode
                    == NonReservedPeerMode::Deny,
            });

            for set_cfg in &network_config.extra_sets {
                let mut reserved_nodes = HashSet::new();
                for reserved in set_cfg.set_config.reserved_nodes.iter() {
                    reserved_nodes.insert(reserved.peer_id);
                    known_addresses.push((reserved.peer_id, reserved.multiaddr.clone()));
                }

                let reserved_only =
                    set_cfg.set_config.non_reserved_mode == NonReservedPeerMode::Deny;

                sets.push(sc_peerset::SetConfig {
                    in_peers: set_cfg.set_config.in_peers,
                    out_peers: set_cfg.set_config.out_peers,
                    bootnodes: Vec::new(),
                    reserved_nodes,
                    reserved_only,
                });
            }

            sc_peerset::Peerset::from_config(sc_peerset::PeersetConfig { sets })
        };

        let behaviour = {
            Notifications::new(
                peerset,
                // NOTE: Block announcement protocol is still very much hardcoded into `Protocol`.
                // 	This protocol must be the first notification protocol given to
                // `Notifications`
                iter::once(notifications::ProtocolConfig {
                    name: block_announces_protocol.notifications_protocol.clone(),
                    fallback_names: block_announces_protocol.fallback_names.clone(),
                    handshake: block_announces_protocol
                        .handshake
                        .as_ref()
                        .map_or(roles.encode(), |h| (*h).to_vec()),
                    max_notification_size: block_announces_protocol.max_notification_size,
                })
                .chain(network_config.extra_sets.iter().map(|s| {
                    notifications::ProtocolConfig {
                        name: s.notifications_protocol.clone(),
                        fallback_names: s.fallback_names.clone(),
                        handshake: s
                            .handshake
                            .as_ref()
                            .map_or(roles.encode(), |h| (*h).to_vec()),
                        max_notification_size: s.max_notification_size,
                    }
                })),
            )
        };

        let protocol = Self {
            pending_messages: VecDeque::new(),
            peerset_handle: peerset_handle.clone(),
            behaviour,
            notification_protocols: iter::once(block_announces_protocol.notifications_protocol)
                .chain(
                    network_config
                        .extra_sets
                        .iter()
                        .map(|s| s.notifications_protocol.clone()),
                )
                .collect(),
        };

        Ok((protocol, peerset_handle, known_addresses))
    }

    /// Disconnects the given peer if we are connected to it.
    #[allow(unused)]
    pub fn disconnect_peer(&mut self, peer_id: &PeerId, protocol_name: ProtocolName) {
        if let Some(position) = self
            .notification_protocols
            .iter()
            .position(|p| *p == protocol_name)
        {
            self.behaviour
                .disconnect_peer(peer_id, sc_peerset::SetId::from(position));
        } else {
            warn!(target: "sub-libp2p", "disconnect_peer() with invalid protocol name")
        }
    }

    /// Adjusts the reputation of a node.
    pub fn report_peer(&self, who: PeerId, reputation: sc_peerset::ReputationChange) {
        self.peerset_handle.report_peer(who, reputation)
    }
}

/// Outcome of an incoming custom message.
#[derive(Debug)]
#[must_use]
pub enum CustomMessageOutcome {
    /// Notification protocols have been opened with a remote.
    NotificationStreamOpened {
        remote: PeerId,
        protocol: ProtocolName,
        negotiated_fallback: Option<ProtocolName>,
        notifications_sink: NotificationsSink,
        handshake: Vec<u8>,
    },
    /// The [`NotificationsSink`] of some notification protocols need an update.
    NotificationStreamReplaced {
        remote: PeerId,
        protocol: ProtocolName,
        notifications_sink: NotificationsSink,
    },
    /// Notification protocols have been closed with a remote.
    NotificationStreamClosed {
        remote: PeerId,
        protocol: ProtocolName,
    },
    /// Messages have been received on one or more notifications protocols.
    NotificationsReceived {
        remote: PeerId,
        messages: Vec<(ProtocolName, Bytes)>,
    },
    /// Interface bound to a peer.
    InterfaceBound { peer: PeerId },
    /// Interface unbound.
    InterfaceUnbound,
}

impl NetworkBehaviour for Protocol {
    type ConnectionHandler = <Notifications as NetworkBehaviour>::ConnectionHandler;
    type OutEvent = CustomMessageOutcome;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        self.behaviour.new_handler()
    }

    fn addresses_of_peer(&mut self, _: &PeerId) -> Vec<Multiaddr> {
        // Only `Discovery::addresses_of_peer` must be returning addresses to ensure that we
        // don't return unwanted addresses.
        Vec::new()
    }

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
        self.behaviour.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <<Self::ConnectionHandler as IntoConnectionHandler>::Handler as
		ConnectionHandler>::OutEvent,
    ) {
        self.behaviour
            .on_connection_handler_event(peer_id, connection_id, event);
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context,
        params: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
        if let Some(message) = self.pending_messages.pop_front() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(message));
        }

        let event = match self.behaviour.poll(cx, params) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev)) => ev,
            Poll::Ready(NetworkBehaviourAction::Dial { opts, handler }) => {
                return Poll::Ready(NetworkBehaviourAction::Dial { opts, handler })
            }
            Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                peer_id,
                handler,
                event,
            }) => {
                return Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                    peer_id,
                    handler,
                    event,
                })
            }
            Poll::Ready(NetworkBehaviourAction::ReportObservedAddr { address, score }) => {
                return Poll::Ready(NetworkBehaviourAction::ReportObservedAddr { address, score })
            }
            Poll::Ready(NetworkBehaviourAction::CloseConnection {
                peer_id,
                connection,
            }) => {
                return Poll::Ready(NetworkBehaviourAction::CloseConnection {
                    peer_id,
                    connection,
                })
            }
        };

        let outcome = match event {
            NotificationsOut::CustomProtocolOpen {
                peer_id,
                set_id,
                received_handshake,
                notifications_sink,
                negotiated_fallback,
            } => {
                log::info!(
                    target: "sub-libp2p",
                    "notification stream opened for {}, peer {peer_id}",
                    self.notification_protocols[usize::from(set_id)]
                );

                CustomMessageOutcome::NotificationStreamOpened {
                    remote: peer_id,
                    protocol: self.notification_protocols[usize::from(set_id)].clone(),
                    negotiated_fallback,
                    notifications_sink,
                    handshake: received_handshake,
                }
            }
            NotificationsOut::CustomProtocolReplaced {
                peer_id,
                notifications_sink,
                set_id,
            } => {
                log::info!(
                    target: "sub-libp2p",
                    "notification stream replaced for {}, peer {peer_id}",
                    self.notification_protocols[usize::from(set_id)]
                );

                CustomMessageOutcome::NotificationStreamReplaced {
                    remote: peer_id,
                    protocol: self.notification_protocols[usize::from(set_id)].clone(),
                    notifications_sink,
                }
            }
            NotificationsOut::CustomProtocolClosed { peer_id, set_id } => {
                log::info!(
                    target: "sub-libp2p",
                    "notification stream closed for {}, peer {peer_id}",
                    self.notification_protocols[usize::from(set_id)]
                );

                CustomMessageOutcome::NotificationStreamClosed {
                    remote: peer_id,
                    protocol: self.notification_protocols[usize::from(set_id)].clone(),
                }
            }
            NotificationsOut::Notification {
                peer_id,
                set_id,
                message,
            } => {
                log::info!(
                    target: "sub-libp2p",
                    "notification received for {}, peer {peer_id}",
                    self.notification_protocols[usize::from(set_id)]
                );

                CustomMessageOutcome::NotificationsReceived {
                    remote: peer_id,
                    messages: vec![(
                        self.notification_protocols[usize::from(set_id)].clone(),
                        message.freeze(),
                    )],
                }
            }
            NotificationsOut::InterfaceBound { peer } => {
                CustomMessageOutcome::InterfaceBound { peer }
            }
            NotificationsOut::InterfaceUnbound => CustomMessageOutcome::InterfaceUnbound,
        };

        Poll::Ready(NetworkBehaviourAction::GenerateEvent(outcome))
    }
}
