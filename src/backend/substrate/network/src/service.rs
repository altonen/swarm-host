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

#![allow(unused)]

// TODO: this code is absolutely hideous, fix it
// TODO: add bunch of more traces
// TODO: start using `tracing`
// TODO: convert to a crate
// TODO: think more about the caching
// TODO: caching random requests is too expensive
// TODO: implement very simple block request handling?
// TODO: relay only the first `ConnectionEstablished` to `overseer`

use crate::{
    behaviour::{self, Behaviour, BehaviourOut},
    config::{self, NetworkConfiguration, NodeKeyConfig, Secret},
    discovery::DiscoveryConfig,
    protocol::{self, NotificationsSink, Protocol},
    transport,
};

use futures::{channel, prelude::*, stream::FuturesUnordered, FutureExt, Stream, StreamExt};
use libp2p::{
    core::upgrade,
    identify::Info as IdentifyInfo,
    identity::ed25519,
    swarm::{AddressScore, ConnectionLimits, Executor, Swarm, SwarmBuilder, SwarmEvent},
    PeerId,
};
use log::{debug, error, info, trace, warn};
use sc_network_common::{
    config::{
        NonDefaultSetConfig, NonReservedPeerMode, NotificationHandshake, ProtocolId, SetConfig,
        TransportConfig,
    },
    error::Error,
    protocol::{role::Role, ProtocolName},
    request_responses::{
        IfDisconnected, IncomingRequest, OutgoingResponse, ProtocolConfig, RequestFailure,
    },
};
use sp_core::H256;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{wrappers::ReceiverStream, StreamMap};

use std::{
    cmp,
    collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque},
    fs,
    hash::Hasher,
    iter,
    num::NonZeroUsize,
    pin::Pin,
    str::FromStr,
    time::Duration,
};

pub use behaviour::{InboundFailure, OutboundFailure, ResponseFailure};
pub use libp2p::identity::{error::DecodingError, Keypair, PublicKey};

const DEFAULT_CHANNEL_SIZE: usize = 64usize;

type PendingResponse = Pin<
    Box<
        dyn Future<
                Output = (
                    PeerId,
                    ProtocolName,
                    usize,
                    Result<Result<Vec<u8>, RequestFailure>, channel::oneshot::Canceled>,
                ),
            > + Send,
    >,
>;

/// Substrate network events.
#[derive(Debug)]
pub enum SubstrateNetworkEvent {
    /// Peer connected.
    PeerConnected { peer: PeerId },
    /// Peer disconnected.
    PeerDisconnected { peer: PeerId },
    /// Peer opened a protocol.
    ProtocolOpened {
        peer: PeerId,
        protocol: ProtocolName,
    },
    /// Peer closed a protocol.
    ProtocolClosed {
        peer: PeerId,
        protocol: ProtocolName,
    },
    /// Notification received from peer.
    NotificationReceived {
        peer: PeerId,
        protocol: ProtocolName,
        notification: Vec<u8>,
    },
    RequestReceived {
        peer: PeerId,
        protocol: ProtocolName,
        request_id: usize,
        request: Vec<u8>,
    },
    ResponseReceived {
        peer: PeerId,
        protocol: ProtocolName,
        request_id: usize,
        response: Vec<u8>,
    },
}

#[derive(Debug)]
pub enum Command {
    SendNotification {
        peer: PeerId,
        protocol: ProtocolName,
        message: Vec<u8>,
    },
    SendRequest {
        peer: PeerId,
        protocol: ProtocolName,
        request: Vec<u8>,
        tx: oneshot::Sender<usize>,
    },
    SendResponse {
        peer: PeerId,
        request_id: usize,
        response: Vec<u8>,
    },
}

pub enum NodeType {
    Masquerade,
    NodeBacked {
        genesis_hash: Vec<u8>,
        role: Role,
        block_announce_config: NonDefaultSetConfig,
    },
}

/// Substrate network
pub struct SubstrateNetwork {
    swarm: Swarm<Behaviour>,
    event_tx: mpsc::Sender<SubstrateNetworkEvent>,
    command_rx: mpsc::Receiver<Command>,
    notification_sinks: HashMap<(PeerId, ProtocolName), NotificationsSink>,
    map: StreamMap<ProtocolName, Pin<Box<dyn futures::Stream<Item = IncomingRequest> + Send>>>,
    next_request_id: usize,
    pending_requests: HashMap<usize, (u64, channel::oneshot::Sender<OutgoingResponse>)>,
    pending_responses: FuturesUnordered<PendingResponse>,
    cached_responses: HashMap<u64, Vec<u8>>,
    rename: HashSet<PeerId>,
}

/// Parse a Ed25519 secret key from a hex string into a `sc_network::Secret`.
fn parse_ed25519_secret(hex: &str) -> config::Ed25519Secret {
    let bytes = H256::from_str(hex).unwrap();

    ed25519::SecretKey::from_bytes(bytes)
        .map(Secret::Input)
        .unwrap()
}

impl SubstrateNetwork {
    /// Create new substrate network
    // TODO: pass socket address
    pub fn new(
        node_type: NodeType,
        executor: Box<dyn Fn(Pin<Box<dyn Future<Output = ()> + Send>>) + Send>,
        event_tx: mpsc::Sender<SubstrateNetworkEvent>,
        command_rx: mpsc::Receiver<Command>,
    ) -> Result<Self, Error> {
        let key_config = NodeKeyConfig::Ed25519(parse_ed25519_secret(
            "0000000000000000000000000000000000000000000000000000000000000001",
        ));
        let mut network_config = NetworkConfiguration::with_key(key_config);
        let mut map = StreamMap::new();

        let block_announce_config = Self::build_block_announce_protocol();
        let sync_config = Self::build_sync_protocol(&mut map);
        let state_config = Self::build_state_sync_protocol(&mut map);
        let warp_config = Self::build_warp_sync_protocol(&mut map);
        let light_config = Self::build_light_protocol(&mut map);

        network_config.request_response_protocols.extend(vec![
            sync_config,
            state_config,
            warp_config,
            light_config,
        ]);
        network_config
            .listen_addresses
            .push("/ip6/::1/tcp/8888".parse().unwrap());
        network_config
            .extra_sets
            .push(Self::build_transaction_protocol());
        network_config
            .extra_sets
            .push(Self::build_grandpa_protocol());

        // TODO: get key from somewhere
        let local_identity = network_config.node_key.clone().into_keypair()?;
        let local_public = local_identity.public();
        let local_peer_id = local_public.to_peer_id();

        if let Some(path) = &network_config.net_config_path {
            fs::create_dir_all(path)?;
        }

        // TODO: zzz
        let role = Role::Full;

        let (protocol, peerset_handle, mut known_addresses) =
            Protocol::new(From::from(&role), &network_config, block_announce_config)?;

        for bootnode in network_config.boot_nodes.iter() {
            known_addresses.push((bootnode.peer_id, bootnode.multiaddr.clone()));
        }

        let (mut swarm, _bandwidth): (Swarm<Behaviour>, _) = {
            let user_agent = format!(
                "{} ({})",
                network_config.client_version, network_config.node_name
            );

            let discovery_config = {
                let mut config = DiscoveryConfig::new(local_public.clone());
                config.with_permanent_addresses(known_addresses);
                config.discovery_limit(u64::from(network_config.default_peers_set.out_peers) + 15);
                config.with_kademlia(&ProtocolId::from("sup"));
                // // TODO: add kademlia support for both node types
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
                    }
                    TransportConfig::Normal {
                        enable_mdns,
                        allow_private_ip: allow_private_ipv4,
                        ..
                    } => {
                        config.with_mdns(enable_mdns);
                        config.allow_private_ip(allow_private_ipv4);
                    }
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
                    Err(crate::request_responses::RegisterError::DuplicateProtocol(proto)) => {
                        return Err(Error::DuplicateRequestResponseProtocol { protocol: proto })
                    }
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

        Ok(Self {
            swarm,
            event_tx,
            command_rx,
            map,
            notification_sinks: HashMap::new(),
            next_request_id: 0usize,
            pending_requests: HashMap::new(),
            pending_responses: FuturesUnordered::new(),
            cached_responses: HashMap::new(),
            rename: HashSet::new(),
        })
    }

    /// Build block announce protocol config.
    fn build_block_announce_protocol() -> NonDefaultSetConfig {
        NonDefaultSetConfig {
            notifications_protocol: format!("/sup/block-announces/1",).into(),
            fallback_names: vec![],
            max_notification_size: 8 * 1024 * 1024,
            handshake: None,
            set_config: SetConfig {
                in_peers: 0,
                out_peers: 0,
                reserved_nodes: Vec::new(),
                non_reserved_mode: NonReservedPeerMode::Deny,
            },
        }
    }

    /// Build transactions protocol config.
    fn build_transaction_protocol() -> NonDefaultSetConfig {
        NonDefaultSetConfig {
            notifications_protocol: "/sup/transactions/1".into(),
            fallback_names: vec![],
            max_notification_size: 16 * 1024 * 1024,
            handshake: None,
            set_config: SetConfig {
                in_peers: 40,
                out_peers: 40,
                reserved_nodes: Vec::new(),
                non_reserved_mode: NonReservedPeerMode::Accept,
            },
        }
    }

    /// Build GRANDPA protocol config.
    fn build_grandpa_protocol() -> NonDefaultSetConfig {
        NonDefaultSetConfig {
            notifications_protocol: "/paritytech/grandpa/1".into(),
            fallback_names: vec![],
            max_notification_size: 1024 * 1024,
            handshake: None,
            set_config: SetConfig {
                in_peers: 40,
                out_peers: 40,
                reserved_nodes: Vec::new(),
                non_reserved_mode: NonReservedPeerMode::Accept,
            },
        }
    }

    /// Build the sync request-response protocol.
    fn build_sync_protocol(
        map: &mut StreamMap<
            ProtocolName,
            Pin<Box<dyn futures::Stream<Item = IncomingRequest> + Send>>,
        >,
    ) -> ProtocolConfig {
        let (tx, mut rx) = futures::channel::mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let name = ProtocolName::from("/sup/sync/2");

        let rx = Box::pin(async_stream::stream! {
              while let Some(item) = rx.next().await {
                  yield item;
              }
        }) as Pin<Box<dyn Stream<Item = IncomingRequest> + Send>>;
        map.insert(name.clone(), rx);

        ProtocolConfig {
            name,
            fallback_names: vec![],
            max_request_size: 1024 * 1024,
            max_response_size: 16 * 1024 * 1024,
            request_timeout: Duration::from_secs(20),
            inbound_queue: Some(tx.clone()),
        }
    }

    /// Build state sync protocol config.
    fn build_state_sync_protocol(
        map: &mut StreamMap<
            ProtocolName,
            Pin<Box<dyn futures::Stream<Item = IncomingRequest> + Send>>,
        >,
    ) -> ProtocolConfig {
        let (tx, mut rx) = futures::channel::mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let name = ProtocolName::from("/sup/state/2");

        let rx = Box::pin(async_stream::stream! {
              while let Some(item) = rx.next().await {
                  yield item;
              }
        }) as Pin<Box<dyn Stream<Item = IncomingRequest> + Send>>;
        map.insert(name.clone(), rx);

        ProtocolConfig {
            name,
            fallback_names: vec![],
            max_request_size: 1024 * 1024,
            max_response_size: 16 * 1024 * 1024,
            request_timeout: Duration::from_secs(40),
            inbound_queue: Some(tx.clone()),
        }
    }

    /// Build warp sync request-response protocol.
    fn build_warp_sync_protocol(
        map: &mut StreamMap<
            ProtocolName,
            Pin<Box<dyn futures::Stream<Item = IncomingRequest> + Send>>,
        >,
    ) -> ProtocolConfig {
        let (tx, mut rx) = futures::channel::mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let name = ProtocolName::from("/sup/sync/warp");

        let rx = Box::pin(async_stream::stream! {
              while let Some(item) = rx.next().await {
                  yield item;
              }
        }) as Pin<Box<dyn Stream<Item = IncomingRequest> + Send>>;
        map.insert(name.clone(), rx);

        ProtocolConfig {
            name,
            fallback_names: vec![],
            max_request_size: 32,
            max_response_size: 16 * 1024 * 1024,
            request_timeout: Duration::from_secs(10),
            inbound_queue: Some(tx.clone()),
        }
    }

    /// Build light request-response protocol.
    fn build_light_protocol(
        map: &mut StreamMap<
            ProtocolName,
            Pin<Box<dyn futures::Stream<Item = IncomingRequest> + Send>>,
        >,
    ) -> ProtocolConfig {
        let (tx, mut rx) = futures::channel::mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let name = ProtocolName::from("/sup/light/2");

        let rx = Box::pin(async_stream::stream! {
              while let Some(item) = rx.next().await {
                  yield item;
              }
        }) as Pin<Box<dyn Stream<Item = IncomingRequest> + Send>>;
        map.insert(name.clone(), rx);

        ProtocolConfig {
            name,
            fallback_names: vec![],
            max_request_size: 1 * 1024 * 1024,
            max_response_size: 16 * 1024 * 1024,
            request_timeout: Duration::from_secs(15),
            inbound_queue: Some(tx),
        }
    }

    /// Get next request ID.
    fn next_request_id(&mut self) -> usize {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    /// Handle command received from `swarm-host`.
    async fn on_command(&mut self, command: Command) {
        match command {
            Command::SendNotification {
                peer,
                protocol,
                message,
            } => {
                if let Some(sink) = self.notification_sinks.get(&(peer, protocol)) {
                    sink.send_sync_notification(message);
                }
            }
            Command::SendRequest {
                peer,
                protocol,
                request,
                tx,
            } => {
                let request_id = self.next_request_id();
                let (res_tx, res_rx) = futures::channel::oneshot::channel();

                log::debug!(
                    target: "sub-libp2p",
                    "send request, peer {peer}, protocol {protocol:?} request id {request_id}"
                );

                // TODO: here check if the request has already been answered by the
                //       the bound peer and if so, feed the response from the cache.
                // TODO: `overseer` needs to know about the caching that's happening
                //       in the backend or otherwise there is useless work being done.
                let digest = {
                    let mut hasher = DefaultHasher::new();
                    hasher.write(&request);
                    hasher.finish()
                };
                tx.send(request_id);

                match self.cached_responses.get(&digest) {
                    Some(response) => {
                        self.event_tx
                            .send(SubstrateNetworkEvent::ResponseReceived {
                                peer,
                                protocol,
                                request_id,
                                response: response.clone(),
                            })
                            .await
                            .expect("channel to stay open");
                    }
                    None => {
                        self.swarm.behaviour_mut().send_request(
                            &peer,
                            &protocol,
                            request,
                            res_tx,
                            IfDisconnected::ImmediateError,
                        );
                        self.pending_responses.push(Box::pin(async move {
                            (peer, protocol, request_id, res_rx.await)
                        }));
                    }
                }
            }
            Command::SendResponse {
                peer,
                request_id,
                response,
            } => {
                log::debug!(target: "sub-libp2p", "send response, peer {peer}, request id {request_id}");

                match self.pending_requests.remove(&request_id) {
                    Some((digest, pending_response)) => {
                        self.cached_responses.insert(digest, response.clone());
                        pending_response.send(OutgoingResponse {
                            result: Ok(response),
                            reputation_changes: vec![],
                            sent_feedback: None,
                        });
                    }
                    None => {
                        log::error!("response channel for request {request_id} no longer exists")
                    }
                }
            }
        }
    }

    /// Handle custom `Swarm` event.
    async fn on_swarm_event(&mut self, event: BehaviourOut) {
        match event {
            BehaviourOut::InboundRequest {
                protocol: _,
                result: _,
                ..
            } => {
                // println!("metrics");
            }
            BehaviourOut::RequestFinished {
                protocol: _,
                duration: _,
                result: _,
                ..
            } => {
                // println!("metrics");
            }
            BehaviourOut::ReputationChanges { peer, changes } => {
                for change in changes {
                    self.swarm
                        .behaviour()
                        .user_protocol()
                        .report_peer(peer, change);
                }
            }
            BehaviourOut::PeerIdentify {
                peer_id,
                info:
                    IdentifyInfo {
                        protocol_version,
                        agent_version,
                        mut listen_addrs,
                        protocols,
                        ..
                    },
            } => {
                // TODO: inform `Overseer` maybe
                for addr in listen_addrs {
                    self.swarm
                        .behaviour_mut()
                        .add_self_reported_address_to_dht(&peer_id, &protocols, addr);
                }
            }
            BehaviourOut::Discovered(peer_id) => {
                // TODO: inform `Overseer`
            }
            BehaviourOut::RandomKademliaStarted => {}
            BehaviourOut::NotificationStreamOpened {
                remote,
                protocol,
                negotiated_fallback,
                notifications_sink,
                handshake,
            } => {
                log::info!("notification stream opened: {protocol}");

                // TODO: save notification sink
                self.notification_sinks
                    .insert((remote, protocol.clone()), notifications_sink);

                self.event_tx
                    .send(SubstrateNetworkEvent::ProtocolOpened {
                        peer: remote,
                        protocol,
                    })
                    .await
                    .expect("channel to stay open");
            }
            BehaviourOut::NotificationStreamReplaced {
                remote: _,
                protocol: _,
                notifications_sink: _,
            } => {
                // TODO: zzzz
                todo!("implement this maybe");
            }
            BehaviourOut::NotificationStreamClosed { remote, protocol } => {
                self.notification_sinks.remove(&(remote, protocol.clone()));

                self.event_tx
                    .send(SubstrateNetworkEvent::ProtocolClosed {
                        peer: remote,
                        protocol,
                    })
                    .await
                    .expect("channel to stay open");
            }
            BehaviourOut::NotificationsReceived { remote, messages } => {
                log::trace!(target: "sub-libp2p", "notification received");

                for (protocol, bytes) in messages {
                    self.event_tx
                        .send(SubstrateNetworkEvent::NotificationReceived {
                            peer: remote,
                            protocol,
                            notification: bytes.into(),
                        })
                        .await
                        .expect("channel to stay open");
                }
            }
            _ => {}
        }
    }

    /// Run the event loop of substrate network
    pub async fn run(mut self) {
        // TODO: ...
        self.pending_responses.push(Box::pin(async move {
            (
                PeerId::random(),
                ProtocolName::from("test"),
                0usize,
                futures::future::pending::<
                    Result<Result<Vec<u8>, RequestFailure>, channel::oneshot::Canceled>,
                >()
                .await,
            )
        }));

        loop {
            tokio::select! {
                event = self.map.next() => match event {
                    Some((protocol, request)) => {
                        let request_id = self.next_request_id();
                        log::warn!(
                            target: "sub-libp2p",
                            "request received peer {}, protocol {:?}, {request_id}",
                            request.peer,
                            protocol,
                        );
                        let digest = {
                            let mut hasher = DefaultHasher::new();
                            hasher.write(&request.payload);
                            hasher.finish()
                        };

                        log::debug!(
                            target: "sub-libp2p",
                            "send request to overseer for further processing"
                        );
                        self.event_tx
                            .send(SubstrateNetworkEvent::RequestReceived {
                                peer: request.peer,
                                protocol,
                                request_id,
                                request: request.payload,
                            })
                            .await
                            .expect("channel to stay open");
                        self.pending_requests.insert(request_id, (digest, request.pending_response));
                    },
                    None => panic!("essential task closed"),
                },
                event = self.pending_responses.select_next_some().fuse() => {
                    match event.3 {
                        Ok(Ok(response)) => {
                            self.event_tx.send(SubstrateNetworkEvent::ResponseReceived {
                                peer: event.0,
                                protocol: event.1,
                                request_id: event.2,
                                response,
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        error => log::error!(
                            target: "sub-libp2p",
                            "failed to receive response to request: {error:?}"
                        ),
                    }
                }
                event = self.command_rx.recv() => match event {
                    Some(command) => self.on_command(command).await,
                    None => panic!("essential task closed"),
                },
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(event) => self.on_swarm_event(event).await,
                    SwarmEvent::ConnectionEstablished {
                        peer_id,
                        endpoint: _,
                        num_established: _,
                        concurrent_dial_errors,
                    } => {
                        if let Some(errors) = concurrent_dial_errors {
                            debug!(
                                target: "sub-libp2p",
                                "Libp2p => Connected({:?}) with errors: {:?}",
                                peer_id,
                                errors
                            );
                        } else {
                            debug!(target: "sub-libp2p", "Libp2p => Connected({:?})", peer_id);
                        }

                        // TODO: if this is another connection from peer, don't update bound peer information
                        if !self.rename.insert(peer_id) {
                            continue
                        }

                        // TODO: this whole code is very dubious
                        self.event_tx.send(SubstrateNetworkEvent::PeerConnected {
                            peer: peer_id,
                        })
                        .await
                        .expect("channel to stay open");

                    }
                    SwarmEvent::ConnectionClosed {
                        peer_id,
                        cause,
                        endpoint: _,
                        num_established: _,
                    } => {
                        debug!(
                            target: "sub-libp2p",
                            "Libp2p => Disconnected({:?}, {:?})",
                            peer_id,
                            cause
                        );

                        // TODO: send peerdisconnected event!
                        self.event_tx.send(SubstrateNetworkEvent::PeerDisconnected {
                            peer: peer_id,
                        })
                        .await
                        .expect("channel to stay open");
                        self.rename.remove(&peer_id);
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
                        let addrs = addresses
                            .into_iter()
                            .map(|a| a.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");

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
}
