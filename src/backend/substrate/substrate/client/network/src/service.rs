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

use crate::{
	behaviour::{self, Behaviour, BehaviourOut},
	discovery::DiscoveryConfig,
	protocol::{self, Protocol},
	transport,
};

use futures::prelude::*;
use libp2p::{
	core::upgrade,
	identify::Info as IdentifyInfo,
	swarm::{AddressScore, ConnectionLimits, Executor, Swarm, SwarmBuilder, SwarmEvent},
};
use log::{debug, error, info, trace, warn};
use sc_network_common::{
	config::{NonDefaultSetConfig, TransportConfig},
	error::Error,
	protocol::{event::Event, role::Role},
};
use std::{cmp, fs, iter, num::NonZeroUsize, pin::Pin};

pub use behaviour::{InboundFailure, OutboundFailure, ResponseFailure};

mod metrics;
mod out_events;
#[cfg(test)]
mod tests;

pub use libp2p::identity::{error::DecodingError, Keypair, PublicKey};

pub enum NodeType {
	Masquerade { role: Role, block_announce_config: NonDefaultSetConfig },
	NodeBacked { genesis_hash: Vec<u8>, role: Role, block_announce_config: NonDefaultSetConfig },
}

/// Substrate network
pub struct SubstrateNetwork {
	swarm: Swarm<Behaviour>,
	/// Senders for events that happen on the network.
	event_streams: out_events::OutChannels,
}

impl SubstrateNetwork {
	/// Create new substrate network
	pub fn new(
		network_config: &crate::config::NetworkConfiguration,
		node_type: NodeType,
		executor: Box<dyn Fn(Pin<Box<dyn Future<Output = ()> + Send>>) + Send>,
	) -> Result<Self, Error> {
		let local_identity = network_config.node_key.clone().into_keypair()?;
		let local_public = local_identity.public();
		let local_peer_id = local_public.to_peer_id();

		if let Some(path) = &network_config.net_config_path {
			fs::create_dir_all(path)?;
		}

		let (role, block_announce_config, _genesis_hash) = match node_type {
			NodeType::Masquerade { role, block_announce_config } =>
				(role, block_announce_config, None),
			NodeType::NodeBacked { genesis_hash, role, block_announce_config } =>
				(role, block_announce_config, Some(genesis_hash)),
		};

		let (protocol, peerset_handle, mut known_addresses) =
			Protocol::new(From::from(&role), &network_config, block_announce_config)?;

		for bootnode in network_config.boot_nodes.iter() {
			known_addresses.push((bootnode.peer_id, bootnode.multiaddr.clone()));
		}

		let (mut swarm, _bandwidth): (Swarm<Behaviour>, _) = {
			let user_agent =
				format!("{} ({})", network_config.client_version, network_config.node_name);

			let discovery_config = {
				let mut config = DiscoveryConfig::new(local_public.clone());
				config.with_permanent_addresses(known_addresses);
				config.discovery_limit(u64::from(network_config.default_peers_set.out_peers) + 15);
				// TODO: add kademlia support for both node types
				// if let Some(genesis_hash) = genesis_hash {
				// 	config.with_kademlia(genesis_hash, fork_id.as_deref(), &protocol_id);
				// }
				config.with_dht_random_walk(network_config.enable_dht_random_walk);
				config.allow_non_globals_in_dht(network_config.allow_non_globals_in_dht);
				config.use_kademlia_disjoint_query_paths(
					network_config.kademlia_disjoint_query_paths,
				);

				match network_config.transport {
					TransportConfig::MemoryOnly => {
						config.with_mdns(false);
						config.allow_private_ip(false);
					},
					TransportConfig::Normal {
						enable_mdns,
						allow_private_ip: allow_private_ipv4,
						..
					} => {
						config.with_mdns(enable_mdns);
						config.allow_private_ip(allow_private_ipv4);
					},
				}

				config
			};

			let (transport, bandwidth) = {
				let yamux_maximum_buffer_size = {
					let requests_max = network_config
						.request_response_protocols
						.iter()
						.map(|cfg| usize::try_from(cfg.max_request_size).unwrap_or(usize::MAX));
					let responses_max = network_config
						.request_response_protocols
						.iter()
						.map(|cfg| usize::try_from(cfg.max_response_size).unwrap_or(usize::MAX));
					let notifs_max = network_config.extra_sets.iter().map(|cfg| {
						usize::try_from(cfg.max_notification_size).unwrap_or(usize::MAX)
					});

					let default_max = cmp::max(
						1024 * 1024,
						usize::try_from(protocol::BLOCK_ANNOUNCES_TRANSACTIONS_SUBSTREAM_SIZE)
							.unwrap_or(usize::MAX),
					);

					iter::once(default_max)
						.chain(requests_max)
						.chain(responses_max)
						.chain(notifs_max)
						.max()
						.expect("iterator known to always yield at least one element; qed")
						.saturating_add(10)
				};

				log::info!(target: "sub-libp2p", "local peer id: {local_peer_id}");

				transport::build_transport(
					local_identity.clone(),
					false,
					network_config.yamux_window_size,
					yamux_maximum_buffer_size,
				)
			};

			let behaviour = {
				let result = Behaviour::new(
					protocol,
					user_agent,
					local_public,
					discovery_config,
					network_config.request_response_protocols.clone(),
					peerset_handle.clone(),
				);

				match result {
					Ok(b) => b,
					Err(crate::request_responses::RegisterError::DuplicateProtocol(proto)) =>
						return Err(Error::DuplicateRequestResponseProtocol { protocol: proto }),
				}
			};

			let builder = {
				struct SpawnImpl<F>(F);
				impl<F: Fn(Pin<Box<dyn Future<Output = ()> + Send>>)> Executor for SpawnImpl<F> {
					fn exec(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) {
						(self.0)(f)
					}
				}
				SwarmBuilder::with_executor(
					transport,
					behaviour,
					local_peer_id,
					SpawnImpl(executor),
				)
			};
			let builder = builder
				.connection_limits(
					ConnectionLimits::default()
						.with_max_established_per_peer(Some(crate::MAX_CONNECTIONS_PER_PEER as u32))
						.with_max_established_incoming(Some(
							crate::MAX_CONNECTIONS_ESTABLISHED_INCOMING,
						)),
				)
				.substream_upgrade_protocol_override(upgrade::Version::V1Lazy)
				.notify_handler_buffer_size(NonZeroUsize::new(32).expect("32 != 0; qed"))
				.connection_event_buffer_size(1024)
				.max_negotiating_inbound_streams(2048);

			(builder.build(), bandwidth)
		};

		for addr in &network_config.listen_addresses {
			if let Err(err) = Swarm::<Behaviour>::listen_on(&mut swarm, addr.clone()) {
				warn!(target: "sub-libp2p", "Can't listen on {} because: {:?}", addr, err)
			}
		}

		for addr in &network_config.public_addresses {
			Swarm::<Behaviour>::add_external_address(
				&mut swarm,
				addr.clone(),
				AddressScore::Infinite,
			);
		}

		Ok(Self { swarm, event_streams: out_events::OutChannels::new(None)? })
	}

	/// Run the event loop of substrate network
	pub async fn run(mut self) {
		loop {
			match self.swarm.select_next_some().await {
				SwarmEvent::Behaviour(BehaviourOut::InboundRequest {
					protocol: _,
					result: _,
					..
				}) => {
					println!("metrics");
				},
				SwarmEvent::Behaviour(BehaviourOut::RequestFinished {
					protocol: _,
					duration: _,
					result: _,
					..
				}) => {
					println!("metrics");
				},
				SwarmEvent::Behaviour(BehaviourOut::ReputationChanges { peer, changes }) => {
					for change in changes {
						self.swarm.behaviour().user_protocol().report_peer(peer, change);
					}
				},
				SwarmEvent::Behaviour(BehaviourOut::PeerIdentify {
					peer_id,
					info:
						IdentifyInfo {
							protocol_version,
							agent_version,
							mut listen_addrs,
							protocols,
							..
						},
				}) => {
					if listen_addrs.len() > 30 {
						debug!(
							target: "sub-libp2p",
							"Node {:?} has reported more than 30 addresses; it is identified by {:?} and {:?}",
							peer_id, protocol_version, agent_version
						);
						listen_addrs.truncate(30);
					}
					for addr in listen_addrs {
						self.swarm
							.behaviour_mut()
							.add_self_reported_address_to_dht(&peer_id, &protocols, addr);
					}
				},
				SwarmEvent::Behaviour(BehaviourOut::Discovered(_peer_id)) => {
					println!("implement maybe, peer id {_peer_id}");
					// self.swarm
					// 	.behaviour_mut()
					// 	.user_protocol_mut()
					// 	.add_default_set_discovered_nodes(iter::once(peer_id));
				},
				SwarmEvent::Behaviour(BehaviourOut::RandomKademliaStarted) => {
					println!("metrics")
				},
				SwarmEvent::Behaviour(BehaviourOut::NotificationStreamOpened {
					remote,
					protocol,
					negotiated_fallback,
					notifications_sink: _,
					handshake,
				}) => {
					log::info!("notification stream opened: {protocol}");
					// TODO: fix this
					// {
					// 	let mut peers_notifications_sinks = this.peers_notifications_sinks.lock();
					// 	let _previous_value = peers_notifications_sinks
					// 		.insert((remote, protocol.clone()), notifications_sink);
					// 	debug_assert!(_previous_value.is_none());
					// }
					self.event_streams.send(Event::NotificationStreamOpened {
						remote,
						protocol,
						negotiated_fallback,
						handshake,
					});
				},
				SwarmEvent::Behaviour(BehaviourOut::NotificationStreamReplaced {
					remote: _,
					protocol: _,
					notifications_sink: _,
				}) => {
					// TODO: fix this
					// let mut peers_notifications_sinks = this.peers_notifications_sinks.lock();
					// if let Some(s) = peers_notifications_sinks.get_mut(&(remote, protocol)) {
					// 	*s = notifications_sink;
					// } else {
					// 	error!(
					// 		target: "sub-libp2p",
					// 		"NotificationStreamReplaced for non-existing substream"
					// 	);
					// 	debug_assert!(false);
					// }

					// TODO: Notifications might have been lost as a result of the previous
					// connection being dropped, and as a result it would be preferable to notify
					// the users of this fact by simulating the substream being closed then
					// reopened.
					// The code below doesn't compile because `role` is unknown. Propagating the
					// handshake of the secondary connections is quite an invasive change and
					// would conflict with https://github.com/paritytech/substrate/issues/6403.
					// Considering that dropping notifications is generally regarded as
					// acceptable, this bug is at the moment intentionally left there and is
					// intended to be fixed at the same time as
					// https://github.com/paritytech/substrate/issues/6403.
					// this.event_streams.send(Event::NotificationStreamClosed {
					// remote,
					// protocol,
					// });
					// this.event_streams.send(Event::NotificationStreamOpened {
					// remote,
					// protocol,
					// role,
					// });
				},
				SwarmEvent::Behaviour(BehaviourOut::NotificationStreamClosed {
					remote,
					protocol,
				}) => {
					self.event_streams.send(Event::NotificationStreamClosed {
						remote,
						protocol: protocol.clone(),
					});
					{
						// TODO: implement this
						// let mut peers_notifications_sinks =
						// this.peers_notifications_sinks.lock(); let _previous_value =
						// peers_notifications_sinks.remove(&(remote, protocol)); debug_assert!
						// (_previous_value.is_some());
					}
				},
				SwarmEvent::Behaviour(BehaviourOut::NotificationsReceived { remote, messages }) => {
					log::info!("notification received: {remote}");
					self.event_streams.send(Event::NotificationsReceived { remote, messages });
				},
				SwarmEvent::Behaviour(BehaviourOut::SyncConnected(remote)) => {
					log::info!("sync connected");
					self.event_streams.send(Event::SyncConnected { remote });
				},
				SwarmEvent::Behaviour(BehaviourOut::SyncDisconnected(remote)) => {
					log::info!("sync disconnected");
					self.event_streams.send(Event::SyncDisconnected { remote });
				},
				SwarmEvent::Behaviour(BehaviourOut::Dht(event, _duration)) => {
					self.event_streams.send(Event::Dht(event));
				},
				SwarmEvent::Behaviour(BehaviourOut::None) => {
					// Ignored event from lower layers.
				},
				SwarmEvent::ConnectionEstablished {
					peer_id,
					endpoint: _,
					num_established: _,
					concurrent_dial_errors,
				} =>
					if let Some(errors) = concurrent_dial_errors {
						debug!(target: "sub-libp2p", "Libp2p => Connected({:?}) with errors: {:?}", peer_id,
		errors);
					} else {
						debug!(target: "sub-libp2p", "Libp2p => Connected({:?})", peer_id);
					},
				SwarmEvent::ConnectionClosed {
					peer_id,
					cause,
					endpoint: _,
					num_established: _,
				} => {
					debug!(target: "sub-libp2p", "Libp2p => Disconnected({:?}, {:?})", peer_id, cause);
				},
				SwarmEvent::NewListenAddr { address, .. } => {
					trace!(target: "sub-libp2p", "Libp2p => NewListenAddr({})", address);
				},
				SwarmEvent::ExpiredListenAddr { address, .. } => {
					info!(target: "sub-libp2p", "📪 No longer listening on {}", address);
				},
				SwarmEvent::OutgoingConnectionError { peer_id, error } => {
					if let Some(peer_id) = peer_id {
						trace!(
							target: "sub-libp2p",
							"Libp2p => Failed to reach {:?}: {}",
							peer_id, error,
						);
					}
				},
				SwarmEvent::Dialing(peer_id) => {
					trace!(target: "sub-libp2p", "Libp2p => Dialing({:?})", peer_id)
				},
				SwarmEvent::IncomingConnection { local_addr, send_back_addr } => {
					trace!(target: "sub-libp2p", "Libp2p => IncomingConnection({},{}))",
						local_addr, send_back_addr);
				},
				SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error } => {
					debug!(
						target: "sub-libp2p",
						"Libp2p => IncomingConnectionError({},{}): {}",
						local_addr, send_back_addr, error,
					);
				},
				SwarmEvent::BannedPeer { peer_id, endpoint } => {
					debug!(
						target: "sub-libp2p",
						"Libp2p => BannedPeer({}). Connected via {:?}.",
						peer_id, endpoint,
					);
				},
				SwarmEvent::ListenerClosed { reason, addresses, .. } => {
					let addrs =
						addresses.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
					match reason {
						Ok(()) => error!(
							target: "sub-libp2p",
							"📪 Libp2p listener ({}) closed gracefully",
							addrs
						),
						Err(e) => error!(
							target: "sub-libp2p",
							"📪 Libp2p listener ({}) closed: {}",
							addrs, e
						),
					}
				},
				SwarmEvent::ListenerError { error, .. } => {
					debug!(target: "sub-libp2p", "Libp2p => ListenerError: {}", error);
				},
			}
		}
	}
}
