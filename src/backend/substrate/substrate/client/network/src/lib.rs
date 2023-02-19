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

#![warn(unused_extern_crates)]

mod behaviour;
mod discovery;
mod peer_info;
mod protocol;
mod request_responses;
mod service;
mod transport;

pub mod config;
pub mod network_state;

#[doc(inline)]
pub use libp2p::{multiaddr, Multiaddr, PeerId};
pub use sc_network_common::{
	protocol::{
		event::{DhtEvent, Event},
		role::ObservedRole,
		ProtocolName,
	},
	request_responses::{IfDisconnected, RequestFailure},
	service::{
		KademliaKey, NetworkBlock, NetworkDHTProvider, NetworkRequest, NetworkSigner,
		NetworkStateInfo, NetworkStatus, NetworkStatusProvider, NetworkSyncForkRequest, Signature,
		SigningError,
	},
	sync::{
		warp::{WarpSyncPhase, WarpSyncProgress},
		StateDownloadProgress, SyncState,
	},
};
pub use service::{
	DecodingError,
	Keypair,
	NodeType,
	OutboundFailure,
	PublicKey,
	SubstrateNetwork,
};

pub use sc_peerset::ReputationChange;

/// The maximum allowed number of established connections per peer.
///
/// Typically, and by design of the network behaviours in this crate,
/// there is a single established connection per peer. However, to
/// avoid unnecessary and nondeterministic connection closure in
/// case of (possibly repeated) simultaneous dialing attempts between
/// two peers, the per-peer connection limit is not set to 1 but 2.
const MAX_CONNECTIONS_PER_PEER: usize = 2;

/// The maximum number of concurrent established connections that were incoming.
const MAX_CONNECTIONS_ESTABLISHED_INCOMING: u32 = 10_000;
