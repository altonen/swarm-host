#![allow(unused)]

use crate::{
    backend::{
        ConnectionUpgrade, IdableRequest, Interface, InterfaceEvent, InterfaceType, NetworkBackend,
        PacketSink,
    },
    ensure,
    error::Error,
    filter::{FilterType, LinkType, MessageFilter, RequestHandlingResult, ResponseHandlingResult},
    types::{OverseerEvent, DEFAULT_CHANNEL_SIZE},
};

use futures::{stream::SelectAll, FutureExt, Stream, StreamExt};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    sync::mpsc::{self, Receiver, Sender},
};

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
    future::Future,
    hash::Hash,
    net::SocketAddr,
    pin::Pin,
};

// TODO: remove `InterfaceId` creation from backend and assign id for it in the overseer
// TODO: get filter type from rpc
// TODO: use spans when function calls `tracing::` multiple times
// TODO: all links should not ben bidrectional
// TODO: split code into functions
// TODO: move tests to separate direcotry
// TODO: add more tetsts
// TODO: print messages and `Vec<u8>` separate and for separate target but in same span
// TODO: convert `overseer` into a module
// TODO: move all request-response handling under some other object that `overseer` owns

/// Logging target for the file.
const LOG_TARGET: &'static str = "overseer";

/// Logging target for binary messages.
const LOG_TARGET_MSG: &'static str = "overseer::msg";

/// Forwarded request.
///
/// Forwarded requests are used with noded-backed interfaces
/// where the [`Overseer`] forwards requests received from other
/// peer to the bound peer. When the response from bound peer is
/// received, the response is forwarded back to the peer where the
/// request originated from, creating the illusion that the interface
/// responded to the request by itself.
///
/// This is done because the interface doesn't keep any storage by itself,
/// and doesn't, for example, use syncing so it's actual blockchain functionality
/// has to be backed by some other peer.
struct ForwardedRequest<T: NetworkBackend> {
    /// Interface where the request originated from.
    source_interface: T::InterfaceId,

    /// Peer who sent the original request.
    source_peer: T::PeerId,

    /// Original request ID.
    request_id: T::RequestId,
}

impl<T: NetworkBackend> ForwardedRequest<T> {
    /// Create new [`ForwardedRequest`].
    pub fn new(
        source_interface: T::InterfaceId,
        source_peer: T::PeerId,
        request_id: T::RequestId,
    ) -> Self {
        Self {
            source_interface,
            source_peer,
            request_id,
        }
    }
}

/// Peer-related information.
struct PeerInfo<T: NetworkBackend> {
    /// Supported protocols.
    protocols: HashSet<T::Protocol>,

    /// Sink for sending messages to peer.
    sink: Box<dyn PacketSink<T> + Send>,

    /// Pending forwarded requests.
    requests: HashMap<T::RequestId, ForwardedRequest<T>>,
}

/// Interface information.
struct InterfaceInfo<T: NetworkBackend> {
    /// Interface handle.
    handle: T::InterfaceHandle,

    /// Interface peers.
    peers: HashMap<T::PeerId, PeerInfo<T>>,

    /// Peer bound to the interface.
    bound_peer: Option<T::PeerId>,
}

impl<T: NetworkBackend> InterfaceInfo<T> {
    /// Create new [`InterfaceInfo`] from `T::InterfaceHandle`.
    pub fn new(handle: T::InterfaceHandle) -> Self {
        Self {
            handle,
            peers: HashMap::new(),
            bound_peer: None,
        }
    }
}

/// Object overseeing `swarm-host` execution.
pub struct Overseer<T: NetworkBackend> {
    /// Network-specific functionality.
    backend: T,

    /// RX channel for receiving events from RPC and peers.
    overseer_rx: Receiver<OverseerEvent<T>>,

    /// TX channel for sending events to [`Overseer`].
    overseer_tx: Sender<OverseerEvent<T>>,

    /// Interfaces.
    interfaces: HashMap<T::InterfaceId, InterfaceInfo<T>>,

    /// Event streams for spawned interfaces.
    event_streams: SelectAll<Pin<Box<dyn Stream<Item = InterfaceEvent<T>> + Send>>>,

    /// Message filters.
    filter: MessageFilter<T>,
}

impl<T: NetworkBackend + Debug> Overseer<T> {
    /// Create new [`Overseer`].
    pub fn new() -> (Self, Sender<OverseerEvent<T>>) {
        let (overseer_tx, overseer_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        (
            Self {
                backend: T::new(),
                overseer_rx,
                event_streams: SelectAll::new(),
                overseer_tx: overseer_tx.clone(),
                filter: MessageFilter::new(),
                interfaces: HashMap::new(),
            },
            overseer_tx,
        )
    }

    /// Start running the [`Overseer`] event loop.
    pub async fn run(mut self) {
        tracing::info!(target: LOG_TARGET, "starting overseer");

        loop {
            tokio::select! {
                result = self.overseer_rx.recv() => match result.expect("channel to stay open") {
                    OverseerEvent::CreateInterface { address, result } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            address = ?address,
                            "create new interface",
                        );

                        match self.backend.spawn_interface(address, InterfaceType::Masquerade).await {
                            Ok((mut handle, event_stream)) => match self.interfaces.entry(*handle.id()) {
                                Entry::Vacant(entry) => {
                                    tracing::trace!(
                                        target: LOG_TARGET,
                                        "interface created"
                                    );

                                    // NOTE: it is logic error for the interface to exist in `MessageFilter`
                                    // if it doesn't exist in `self.interfaces` so `expect` is justified.
                                    self
                                        .filter
                                        .register_interface(*handle.id(), FilterType::FullBypass)
                                        .expect("unique interface");
                                    self.event_streams.push(event_stream);
                                    result.send(Ok(*handle.id())).expect("channel to stay open");
                                    entry.insert(InterfaceInfo::new(handle));
                                },
                                Entry::Occupied(_) => tracing::error!(
                                    target: LOG_TARGET,
                                    id = ?*handle.id(),
                                    "duplicate interface id"
                                ),
                            }
                            Err(err) => {
                                tracing::error!(
                                    target: LOG_TARGET,
                                    error = ?err,
                                    "failed to start interface"
                                );
                            },
                        }
                    }
                    OverseerEvent::LinkInterface { first, second, result } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface = ?first,
                            interface = ?second,
                            "link interfaces",
                        );

                        result.send(self.filter.link_interface(first, second, LinkType::Bidrectional));
                    }
                    OverseerEvent::UnlinkInterface { first, second, result } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface = ?first,
                            interface = ?second,
                            "unlink interfaces",
                        );

                        result.send(self.filter.unlink_interface(first, second));
                    }
                    OverseerEvent::InstallFilter { interface, filter_name, result } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface = ?interface,
                            filter_name = filter_name,
                            "add filter",
                        );

                        let call_result = self.interfaces.get(&interface).map_or(
                            Err(Error::InterfaceDoesntExist),
                            |info| {
                                match info.handle.filter(&filter_name) {
                                    Some(filter) => self.filter.install_filter(interface, filter),
                                    None => Err(Error::FilterDoesntExist),
                                }
                            }
                        );

                        result.send(call_result).expect("channel to stay open");
                    }
                },
                event = self.event_streams.next() => match event {
                    Some(InterfaceEvent::PeerConnected { peer, interface, protocols, sink }) => {
                        if let Err(err) = self.add_peer(interface, peer, protocols, sink) {
                           tracing::warn!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                "peer already exists in the filter",
                            );
                        }
                    }
                    Some(InterfaceEvent::PeerDisconnected { peer, interface }) => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface_id = ?interface,
                            peer_id = ?peer,
                            "peer disconnected"
                        );

                        match self.interfaces.get_mut(&interface) {
                            None => tracing::error!(
                                target: LOG_TARGET,
                                interface = ?interface,
                                peer_id = ?peer,
                                "interface does not exist",
                            ),
                            Some(info) => if info.peers.remove(&peer).is_none() {
                                tracing::warn!(
                                    target: LOG_TARGET,
                                    interface_id = ?interface,
                                    peer_id = ?peer,
                                    "peer does not exist",
                                );
                            }
                        }
                    }
                    Some(InterfaceEvent::MessageReceived { peer, interface, protocol, message }) => {
                        // TODO: span?
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface_id = ?interface,
                            peer_id = ?peer,
                            "message received from peer"
                        );
                        tracing::trace!(
                            target: LOG_TARGET_MSG,
                            message = ?message,
                        );

                        match self.filter.inject_message(
                            interface,
                            peer,
                            &message
                        ) {
                            Ok(routing_table) => for (interface, peer) in routing_table {
                                match self
                                    .interfaces
                                    .get_mut(&interface)
                                    .expect("interface to exist")
                                    .peers
                                    .get_mut(&peer)
                                {
                                    None => tracing::error!(
                                        target: LOG_TARGET,
                                        interface_id = ?interface,
                                        peer_id = ?peer,
                                        "peer does not exist"
                                    ),
                                    Some(peer_info) => {
                                        match peer_info.sink.send_packet(Some(protocol.clone()), &message).await {
                                            Ok(_) =>
                                                tracing::trace!(
                                                    target: LOG_TARGET,
                                                    interface_id = ?interface,
                                                    peer_id = ?peer,
                                                    "message sent to peer",
                                                ),
                                            Err(err) => tracing::error!(
                                                target: LOG_TARGET,
                                                interface_id = ?interface,
                                                peer_id = ?peer,
                                                error = ?err,
                                                "failed to send message"
                                            ),
                                        }
                                    }
                                }
                            }
                            Err(err) => tracing::error!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                err = ?err,
                                "failed to inject message into `MessageFilter`",
                            ),
                        }
                    }
                    Some(InterfaceEvent::RequestReceived { interface, peer, protocol, request }) => {
                        if let Err(err) = self.inject_request(interface, peer, protocol, request).await {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                err = ?err,
                                "failed to inject request into `MessageFilter`",
                            );
                        }
                    },
                    Some(InterfaceEvent::ResponseReceived { interface, peer, protocol, request_id, response }) => {
                        if let Err(err) = self
                            .inject_response(interface, peer, protocol, request_id, response)
                            .await
                        {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                err = ?err,
                                "failed to inject response into `MessageFilter`",
                            );
                        }
                    },
                    Some(InterfaceEvent::ConnectionUpgraded { interface, peer, upgrade }) => {
                        if let Err(err) = self.apply_connection_upgrade(interface, peer, upgrade) {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                error = ?err,
                                "failed to apply connection upgrade",
                            );
                        }
                    },
                    Some(InterfaceEvent::InterfaceBound { interface, peer }) => {
                        if let Err(err) = self.bind_interface(interface, peer) {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                error = ?err,
                                "failed to bind node to interface",
                            );
                        }
                    }
                    Some(InterfaceEvent::InterfaceUnbound { interface }) => {
                        if let Err(err) = self.unbind_interface(interface) {
                            tracing::error!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                error = ?err,
                                "failed to bind node to interface",
                            );
                        }
                    }
                    _ => {},
                }
            }
        }
    }

    fn add_peer(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        protocols: Vec<T::Protocol>,
        sink: Box<dyn PacketSink<T> + Send>,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            interface_id = ?interface,
            peer_id = ?peer,
            "peer connected"
        );

        // TODO: more comprehensive error handling
        match self
            .filter
            .register_peer(interface, peer, FilterType::FullBypass)
        {
            Err(Error::PeerAlreadyExists) => {
                tracing::warn!(
                    target: LOG_TARGET,
                    interface_id = ?interface,
                    peer_id = ?peer,
                    "peer already exists in the filter",
                );
            }
            Ok(_) => {}
            Err(err) => panic!("unrecoverable error occurred: {err:?}"),
        }

        self.interfaces
            .get_mut(&interface)
            .expect("interface to exist")
            .peers
            .insert(
                peer,
                PeerInfo {
                    protocols: HashSet::from_iter(protocols.into_iter()),
                    requests: HashMap::new(),
                    sink,
                },
            );

        Ok(())
    }

    /// Bind node to interface.
    fn bind_interface(&mut self, interface: T::InterfaceId, peer: T::PeerId) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            interface_id = ?interface,
            peer_id = ?peer,
            "interface bound to peer",
        );

        let info = self
            .interfaces
            .get_mut(&interface)
            .ok_or(Error::InterfaceDoesntExist)?;
        ensure!(info.peers.contains_key(&peer), Error::PeerDoesntExist);

        info.bound_peer = Some(peer);
        Ok(())
    }

    /// Unbind interface and release all resources.
    fn unbind_interface(&mut self, interface: T::InterfaceId) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            interface_id = ?interface,
            "unbind interface",
        );

        // TODO: release all resources related to interface binding
        self.interfaces
            .get_mut(&interface)
            .ok_or(Error::InterfaceDoesntExist)?
            .bound_peer = None;
        Ok(())
    }

    /// Inject request to `MessageFilter` and possibly route it to some connected peer.
    async fn inject_request(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        protocol: T::Protocol,
        request: T::Request,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            interface_id = ?interface,
            peer_id = ?peer,
            protocol = ?protocol,
            request_id = ?request.id(),
            "handle request",
        );

        // TODO: zzz
        let (bound_peer, mut bound_peer_info) = {
            let bound_peer = self
                .interfaces
                .get_mut(&interface)
                .expect("interface to exist")
                .bound_peer
                .expect("bound peer to exist");

            let info = self
                .interfaces
                .get_mut(&interface)
                .expect("interface to exist")
                .peers
                .get_mut(&bound_peer)
                .expect("bound peer to exist");

            (bound_peer, info)
        };

        if peer == bound_peer {
            return Err(Error::Custom(format!(
                "Bound peer sent a request: {bound_peer:?}"
            )));
        }

        match self
            .filter
            .inject_request(interface, peer, &protocol, &request)
        {
            RequestHandlingResult::Timeout => todo!("timeouts not implemented"),
            RequestHandlingResult::Reject => todo!("rejects not implemented"),
            RequestHandlingResult::Forward => {
                tracing::trace!(
                    target: LOG_TARGET,
                    interface_id = ?interface,
                    peer_id = ?peer,
                    protocol = ?protocol,
                    bound_peer_id = ?bound_peer,
                    "forward request to bound peer",
                );

                let inbound_request_id = *request.id();
                let outbound_request_id =
                    bound_peer_info.sink.send_request(protocol, request).await?;

                bound_peer_info.requests.insert(
                    outbound_request_id,
                    ForwardedRequest::new(interface, peer, inbound_request_id),
                );
            }
        }

        Ok(())
    }

    /// Inject response to `MessageFilter` and possibly route it to some connected peer.
    async fn inject_response(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        protocol: T::Protocol,
        request_id: T::RequestId,
        response: T::Response,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            interface_id = ?interface,
            peer_id = ?peer,
            protocol = ?protocol,
            "inject response",
        );

        match self
            .filter
            .inject_response(interface, peer, &protocol, &response)
        {
            ResponseHandlingResult::Timeout => todo!("timeouts not implemented"),
            ResponseHandlingResult::Reject => todo!("rejects not implemented"),
            ResponseHandlingResult::Forward => {
                tracing::trace!(
                    target: LOG_TARGET,
                    interface_id = ?interface,
                    peer_id = ?peer,
                    protocol = ?protocol,
                    "inject response",
                );

                let peer_info = self
                    .interfaces
                    .get_mut(&interface)
                    .expect("interface to exist")
                    .peers
                    .get_mut(&peer)
                    .ok_or(Error::PeerDoesntExist)?;

                match peer_info.requests.remove(&request_id) {
                    None => {
                        tracing::warn!(
                            target: LOG_TARGET,
                            interface_id = ?interface,
                            peer_id = ?peer,
                            request_id = ?request_id,
                            "forwarded request does not exist",
                        );
                        Err(Error::RequestDoesntExist)
                    }
                    Some(forwarded_request) => {
                        self.interfaces
                            .get_mut(&forwarded_request.source_interface)
                            .ok_or(Error::InterfaceDoesntExist)?
                            .peers
                            .get_mut(&forwarded_request.source_peer)
                            .ok_or(Error::PeerDoesntExist)?
                            .sink
                            .send_response(forwarded_request.request_id, response)
                            .await
                    }
                }
            }
        }
    }

    /// Apply connection upgrade for an active peer.
    fn apply_connection_upgrade(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        upgrade: ConnectionUpgrade<T>,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            interface_id = ?interface,
            peer_id = ?peer,
            upgrade = ?upgrade,
            "apply upgrade to connection",
        );

        let peer_info = self
            .interfaces
            .get_mut(&interface)
            .expect("interface to exist")
            .peers
            .get_mut(&peer)
            .ok_or(Error::PeerDoesntExist)?;

        match upgrade {
            ConnectionUpgrade::ProtocolOpened { protocols } => {
                peer_info.protocols.extend(protocols)
            }
            ConnectionUpgrade::ProtocolClosed { protocols } => peer_info
                .protocols
                .retain(|protocol| !protocols.contains(protocol)),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mockchain::{
        types::{ProtocolId, Request, Response},
        MockchainBackend, MockchainHandle,
    };
    use rand::Rng;

    // TODO: use `mockall`
    #[derive(Debug)]
    struct DummyHandle;

    #[async_trait::async_trait]
    impl Interface<MockchainBackend> for DummyHandle {
        fn id(&self) -> &<MockchainBackend as NetworkBackend>::InterfaceId {
            todo!();
        }

        fn filter(
            &self,
            filter_name: &String,
        ) -> Option<
            Box<
                dyn Fn(
                        <MockchainBackend as NetworkBackend>::InterfaceId,
                        <MockchainBackend as NetworkBackend>::PeerId,
                        <MockchainBackend as NetworkBackend>::InterfaceId,
                        <MockchainBackend as NetworkBackend>::PeerId,
                        &<MockchainBackend as NetworkBackend>::Message,
                    ) -> bool
                    + Send,
            >,
        > {
            todo!()
        }

        fn connect(&mut self, address: SocketAddr) -> crate::Result<()> {
            todo!();
        }

        fn disconnect(
            &mut self,
            peer: <MockchainBackend as NetworkBackend>::PeerId,
        ) -> crate::Result<()> {
            todo!();
        }
    }

    // TODO: use `mockall`
    #[derive(Debug)]
    struct DummySink<T: NetworkBackend> {
        msg_tx: mpsc::Sender<T::Message>,
        req_tx: mpsc::Sender<T::Request>,
        resp_tx: mpsc::Sender<T::Response>,
    }

    impl DummySink<MockchainBackend> {
        pub fn new() -> (
            Self,
            mpsc::Receiver<<MockchainBackend as NetworkBackend>::Message>,
            mpsc::Receiver<<MockchainBackend as NetworkBackend>::Request>,
            mpsc::Receiver<<MockchainBackend as NetworkBackend>::Response>,
        ) {
            let (msg_tx, msg_rx) = mpsc::channel(64);
            let (req_tx, req_rx) = mpsc::channel(64);
            let (resp_tx, resp_rx) = mpsc::channel(64);

            (
                Self {
                    msg_tx,
                    req_tx,
                    resp_tx,
                },
                msg_rx,
                req_rx,
                resp_rx,
            )
        }
    }

    #[async_trait::async_trait]
    impl PacketSink<MockchainBackend> for DummySink<MockchainBackend> {
        async fn send_packet(
            &mut self,
            protocol: Option<<MockchainBackend as NetworkBackend>::Protocol>,
            message: &<MockchainBackend as NetworkBackend>::Message,
        ) -> crate::Result<()> {
            todo!();
        }

        async fn send_request(
            &mut self,
            protocol: <MockchainBackend as NetworkBackend>::Protocol,
            request: <MockchainBackend as NetworkBackend>::Request,
        ) -> crate::Result<<MockchainBackend as NetworkBackend>::RequestId> {
            self.req_tx.send(request).await.unwrap();
            Ok(0u64)
        }

        async fn send_response(
            &mut self,
            request_id: <MockchainBackend as NetworkBackend>::RequestId,
            response: <MockchainBackend as NetworkBackend>::Response,
        ) -> crate::Result<()> {
            self.resp_tx.send(response).await.unwrap();
            Ok(())
        }
    }

    #[tokio::test]
    async fn apply_connection_upgrade() {
        let mut rng = rand::thread_rng();
        let (mut overseer, _) = Overseer::<MockchainBackend>::new();
        let interface = rng.gen();
        let peer = rng.gen();

        overseer.interfaces.insert(
            interface,
            InterfaceInfo::<MockchainBackend>::new(
                MockchainHandle::new(
                    interface,
                    "[::1]:8888".parse().unwrap(),
                    InterfaceType::Masquerade,
                )
                .await
                .unwrap()
                .0,
            ),
        );

        let (sink, _, _, _) = DummySink::new();
        overseer
            .interfaces
            .get_mut(&interface)
            .unwrap()
            .peers
            .insert(
                peer,
                PeerInfo {
                    protocols: HashSet::from([ProtocolId::Transaction, ProtocolId::Block]),
                    requests: HashMap::new(),
                    sink: Box::new(sink),
                },
            );

        assert_eq!(
            overseer
                .interfaces
                .get(&interface)
                .unwrap()
                .peers
                .get(&peer)
                .unwrap()
                .protocols,
            HashSet::from([ProtocolId::Transaction, ProtocolId::Block]),
        );

        // add new protocol and verify peer protocols are updated
        overseer.apply_connection_upgrade(
            interface,
            peer,
            ConnectionUpgrade::ProtocolOpened {
                protocols: HashSet::from([ProtocolId::Generic]),
            },
        );

        assert_eq!(
            overseer
                .interfaces
                .get(&interface)
                .unwrap()
                .peers
                .get(&peer)
                .unwrap()
                .protocols,
            HashSet::from([
                ProtocolId::Transaction,
                ProtocolId::Block,
                ProtocolId::Generic
            ]),
        );

        // close two protocols: one that's supported and one that's not and verify state again
        overseer.apply_connection_upgrade(
            interface,
            peer,
            ConnectionUpgrade::ProtocolClosed {
                protocols: HashSet::from([ProtocolId::PeerExchange, ProtocolId::Transaction]),
            },
        );

        assert_eq!(
            overseer
                .interfaces
                .get(&interface)
                .unwrap()
                .peers
                .get(&peer)
                .unwrap()
                .protocols,
            HashSet::from([ProtocolId::Block, ProtocolId::Generic]),
        );
    }

    #[tokio::test]
    async fn request_forwarded_to_bound_node() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut rng = rand::thread_rng();
        let (mut overseer, _) = Overseer::<MockchainBackend>::new();
        let interface = rng.gen();
        let peer1 = rng.gen();
        let peer2 = rng.gen();
        let peer3 = rng.gen();

        overseer.interfaces.insert(
            interface,
            InterfaceInfo::<MockchainBackend>::new(
                MockchainHandle::new(
                    interface,
                    "[::1]:0".parse().unwrap(),
                    InterfaceType::Masquerade,
                )
                .await
                .unwrap()
                .0,
            ),
        );
        overseer
            .filter
            .register_interface(interface, FilterType::FullBypass)
            .expect("unique interface");

        let (sink1, _, mut req_rx1, _) = DummySink::new();
        let (sink2, _, _, _) = DummySink::new();
        let (sink3, _, _, mut resp_rx3) = DummySink::new();

        overseer
            .add_peer(
                interface,
                peer1,
                vec![ProtocolId::Transaction, ProtocolId::Block],
                Box::new(sink1),
            )
            .unwrap();
        overseer
            .add_peer(
                interface,
                peer2,
                vec![ProtocolId::Transaction, ProtocolId::Block],
                Box::new(sink2),
            )
            .unwrap();
        overseer
            .add_peer(
                interface,
                peer3,
                vec![ProtocolId::Transaction, ProtocolId::Block],
                Box::new(sink3),
            )
            .unwrap();
        overseer.bind_interface(interface, peer1).unwrap();

        // receive request from `peer3` and verify it's forwarded to `peer1`
        overseer
            .inject_request(
                interface,
                peer3,
                ProtocolId::Block,
                Request::new(0u64, vec![1, 2, 3, 4]),
            )
            .await
            .unwrap();
        assert_eq!(req_rx1.try_recv(), Ok(Request::new(0u64, vec![1, 2, 3, 4])));

        // inject response to request from `peer1` and verify that it is forwarded to `peer3`
        let request_id = overseer
            .interfaces
            .get(&interface)
            .unwrap()
            .peers
            .get(&peer1)
            .unwrap()
            .requests
            .iter()
            .next()
            .unwrap()
            .1
            .request_id;

        overseer
            .inject_response(
                interface,
                peer1,
                ProtocolId::Block,
                request_id,
                Response::new(0u64, vec![5, 6, 7, 8]),
            )
            .await;
        assert_eq!(
            resp_rx3.try_recv(),
            Ok(Response::new(0u64, vec![5, 6, 7, 8]))
        );
    }
}
