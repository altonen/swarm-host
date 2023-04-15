#![allow(unused)]

use crate::{
    backend::{
        ConnectionUpgrade, Idable, Interface, InterfaceEvent, InterfaceType, NetworkBackend,
        PacketSink,
    },
    ensure,
    error::Error,
    executor::Executor,
    filter::{Filter, FilterEvent, FilterHandle},
    types::{OverseerEvent, DEFAULT_CHANNEL_SIZE},
};

use futures::{stream::SelectAll, FutureExt, Stream, StreamExt};
use petgraph::{
    graph::{EdgeIndex, NodeIndex, UnGraph},
    visit::{Dfs, Walker},
};
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

/// Logging target for the file.
const LOG_TARGET: &'static str = "overseer";

/// Logging target for binary messages.
const LOG_TARGET_MSG: &'static str = "overseer::msg";

/// Peer-related information.
struct PeerInfo<T: NetworkBackend> {
    /// Supported protocols.
    protocols: HashSet<T::Protocol>,
}

/// Interface information.
struct InterfaceInfo<T: NetworkBackend> {
    /// Interface handle.
    handle: T::InterfaceHandle,

    /// Interface peers.
    peers: HashMap<T::PeerId, PeerInfo<T>>,

    /// Filter for the interface.
    filter: FilterHandle<T>,

    /// Index to the link graph.
    index: NodeIndex,
}

impl<T: NetworkBackend> InterfaceInfo<T> {
    /// Create new [`InterfaceInfo`] from `T::InterfaceHandle`.
    pub fn new(index: NodeIndex, handle: T::InterfaceHandle, filter: FilterHandle<T>) -> Self {
        Self {
            index,
            handle,
            filter,
            peers: HashMap::new(),
        }
    }
}

/// Object overseeing `swarm-host` execution.
pub struct Overseer<T: NetworkBackend, E: Executor<T>> {
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

    /// Events received from the filters.
    filter_events: mpsc::Receiver<FilterEvent>,

    /// TX channel passed to new `Filter`s.
    filter_event_tx: mpsc::Sender<FilterEvent>,

    /// Links between interfaces.
    links: UnGraph<T::InterfaceId, ()>,

    /// Edges between interfaces.
    edges: HashMap<(T::InterfaceId, T::InterfaceId), EdgeIndex>,

    /// Executor
    _marker: std::marker::PhantomData<E>,
}

impl<T: NetworkBackend, E: Executor<T>> Overseer<T, E> {
    /// Create new [`Overseer`].
    pub fn new() -> (Self, Sender<OverseerEvent<T>>) {
        let (overseer_tx, overseer_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let (filter_event_tx, filter_events) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        (
            Self {
                backend: T::new(),
                overseer_rx,
                event_streams: SelectAll::new(),
                overseer_tx: overseer_tx.clone(),
                links: UnGraph::new_undirected(),
                edges: HashMap::new(),
                interfaces: HashMap::new(),
                filter_events,
                filter_event_tx,
                _marker: Default::default(),
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
                        match self.create_interface(address).await {
                            Ok(interface) => result.send(Ok(interface)).expect("channel to stay open"),
                            Err(err) => {
                                tracing::error!(
                                    target: LOG_TARGET,
                                    ?address,
                                    ?err,
                                    "failed to crate interface",
                                );
                                result.send(Err(err)).expect("channel to stay open");
                            }
                        }
                    }
                    OverseerEvent::LinkInterface { first, second, result } => {
                        result.send(self.link_interfaces(first, second)).expect("channel to stay open");
                    }
                    OverseerEvent::UnlinkInterface { first, second, result } => {
                        result.send(self.unlink_interfaces(first, second));
                    }
                    OverseerEvent::InitializeFilter { interface, code, context, result } => {
                        result.send(self.initialize_filter(interface, code, context).await);
                    }
                    OverseerEvent::InstallNotificationFilter {
                        interface,
                        protocol,
                        filter_code,
                        context,
                        result
                    } => {
                        let result =
                        result
                            .send(
                                self.install_notification_filter(
                                    interface,
                                    protocol,
                                    filter_code,
                                    context,
                                ).await,
                            )
                            .expect("channel to stay open");
                    }
                    OverseerEvent::InstallRequestResponseFilter {
                        interface,
                        protocol,
                        filter_code,
                        context,
                        result
                    } => {
                        let result =
                        result
                            .send(
                                self.install_request_response_filter(
                                    interface,
                                    protocol,
                                    filter_code,
                                    context,
                                ).await,
                            )
                            .expect("channel to stay open");
                    }
                },
                event = self.event_streams.next() => match event {
                    Some(InterfaceEvent::PeerConnected { peer, interface, protocols, sink }) => {
                        if let Err(err) = self.register_peer(interface, peer, protocols, sink).await {
                           tracing::warn!(
                                target: LOG_TARGET,
                                ?interface,
                                ?peer,
                                "failed to register peer",
                            );
                        }
                    }
                    Some(InterfaceEvent::PeerDisconnected { peer, interface }) => {
                        if let Err(err) = self.unregister_peer(interface, peer).await {
                           tracing::warn!(
                                target: LOG_TARGET,
                                ?interface,
                                ?peer,
                                "failed to unregister peer",
                            );
                        }
                    }
                    Some(InterfaceEvent::MessageReceived { interface, peer, protocol, message }) => {
                        if let Err(err) = self.inject_notification(interface, peer, protocol, message).await {
                           tracing::warn!(
                                target: LOG_TARGET,
                                interface_id = ?interface,
                                peer_id = ?peer,
                                "peer already exists in the filter",
                            );
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
                    _ => {},
                },
                event = self.filter_events.recv() => match event.expect("channel to stay open") {
                    _ => todo!(),
                }
            }
        }
    }

    async fn create_interface(&mut self, address: SocketAddr) -> crate::Result<T::InterfaceId> {
        tracing::debug!(
            target: LOG_TARGET,
            address = ?address,
            "create new interface",
        );

        match self
            .backend
            .spawn_interface(address, InterfaceType::Masquerade)
            .await
        {
            Ok((mut handle, event_stream)) => match self.interfaces.entry(*handle.id()) {
                Entry::Vacant(entry) => {
                    let interface_id = *handle.id();
                    let node_index = self.links.add_node(interface_id);
                    let (filter, filter_handle) =
                        Filter::<T, E>::new(interface_id, self.filter_event_tx.clone());

                    tracing::trace!(target: LOG_TARGET, interface = ?interface_id, ?node_index, "interface created");

                    self.event_streams.push(event_stream);
                    entry.insert(InterfaceInfo::new(node_index, handle, filter_handle));
                    tokio::spawn(filter.run());

                    Ok(interface_id)
                }
                Entry::Occupied(_) => Err(Error::InterfaceAlreadyExists),
            },
            Err(err) => Err(err),
        }
    }

    /// Link interfaces together.
    fn link_interfaces(
        &mut self,
        first: T::InterfaceId,
        second: T::InterfaceId,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?first, ?second, "link interfaces");

        match (self.interfaces.get(&first), self.interfaces.get(&second)) {
            (Some(first_info), Some(second_info)) => {
                let edge = self.links.add_edge(first_info.index, second_info.index, ());
                self.edges.insert((first, second), edge);
                Ok(())
            }
            _ => Err(Error::InterfaceDoesntExist),
        }
    }

    /// Unlink interfaces.
    fn unlink_interfaces(
        &mut self,
        first: T::InterfaceId,
        second: T::InterfaceId,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            interface = ?first,
            interface = ?second,
            "unlink interfaces",
        );

        match self.edges.remove(&(first, second)) {
            None => Err(Error::LinkDoesntExist),
            Some(edge) => self
                .links
                .remove_edge(edge)
                .ok_or(Error::LinkDoesntExist)
                .map(|_| ()),
        }
    }

    async fn register_peer(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        protocols: Vec<T::Protocol>,
        sink: Box<dyn PacketSink<T> + Send>,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            ?interface,
            ?peer,
            ?protocols,
            "peer connected"
        );

        match self.interfaces.get_mut(&interface) {
            None => Err(Error::InterfaceDoesntExist),
            Some(info) => {
                info.peers.insert(
                    peer,
                    PeerInfo {
                        protocols: HashSet::from_iter(protocols.into_iter()),
                    },
                );

                // TODO: pass protocols?
                info.filter.register_peer(peer, sink).await;
                Ok(())
            }
        }
    }

    async fn unregister_peer(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?interface, ?peer, "peer disconnected");

        match self.interfaces.get_mut(&interface) {
            None => Err(Error::InterfaceDoesntExist),
            Some(info) => match info.peers.remove(&peer) {
                None => Err(Error::PeerDoesntExist),
                Some(_) => {
                    info.filter.unregister_peer(peer).await;
                    Ok(())
                }
            },
        }
    }

    async fn initialize_filter(
        &mut self,
        interface: T::InterfaceId,
        filter_code: String,
        context: String,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?interface, "initialize filter");

        match self.interfaces.get_mut(&interface) {
            None => Err(Error::InterfaceDoesntExist),
            Some(info) => {
                info.filter
                    .initialize_filter(filter_code, Some(context))
                    .await;
                Ok(())
            }
        }
    }

    async fn install_notification_filter(
        &mut self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        filter_code: String,
        context: String,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            ?interface,
            ?protocol,
            "install notification filter",
        );

        match self.interfaces.get_mut(&interface) {
            None => Err(Error::InterfaceDoesntExist),
            Some(info) => {
                info.filter
                    .install_notification_filter(protocol, filter_code, Some(context))
                    .await;
                Ok(())
            }
        }
    }

    async fn install_request_response_filter(
        &mut self,
        interface: T::InterfaceId,
        protocol: T::Protocol,
        filter_code: String,
        context: String,
    ) -> crate::Result<()> {
        tracing::debug!(
            target: LOG_TARGET,
            ?interface,
            ?protocol,
            "install request-response filter",
        );

        match self.interfaces.get_mut(&interface) {
            None => Err(Error::InterfaceDoesntExist),
            Some(info) => {
                info.filter
                    .install_request_response_filter(protocol, filter_code, Some(context))
                    .await;
                Ok(())
            }
        }
    }

    async fn inject_notification(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        protocol: T::Protocol,
        notification: T::Message,
    ) -> crate::Result<()> {
        tracing::debug!(target: LOG_TARGET, ?interface, ?peer, "inject notification");
        tracing::trace!(target: LOG_TARGET_MSG, ?notification);

        match self.interfaces.get_mut(&interface) {
            None => Err(Error::InterfaceDoesntExist),
            Some(info) => {
                let linked_interfaces = Dfs::new(&self.links, info.index)
                    .iter(&self.links)
                    .map(|nx| self.links.node_weight(nx).expect("entry to exist"))
                    .collect::<Vec<_>>();

                for interface in linked_interfaces {
                    self.interfaces
                        .get_mut(interface)
                        .expect("entry to exist")
                        .filter
                        .inject_notification(protocol.clone(), peer, notification.clone())
                        .await;
                }

                Ok(())
            }
        }
    }

    /// Inject request to `MessageFilter` and possibly route it to some connected peer.
    async fn inject_request(
        &mut self,
        interface: T::InterfaceId,
        peer: T::PeerId,
        protocol: T::Protocol,
        request: T::Request,
    ) -> crate::Result<()> {
        tracing::warn!(
            target: LOG_TARGET,
            ?interface,
            ?peer,
            ?protocol,
            request_id = ?request.id(),
            "inject request",
        );

        match self.interfaces.get_mut(&interface) {
            None => Err(Error::InterfaceDoesntExist),
            Some(info) => {
                info.filter.inject_request(protocol, peer, request).await;
                Ok(())
            }
        }
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
        tracing::warn!(
            target: LOG_TARGET,
            ?interface,
            ?peer,
            ?protocol,
            "inject response",
        );

        match self.interfaces.get_mut(&interface) {
            None => Err(Error::InterfaceDoesntExist),
            Some(info) => {
                info.filter.inject_response(protocol, peer, response).await;
                Ok(())
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
    use crate::{
        backend::mockchain::{
            self,
            types::{ProtocolId, Request, RequestId, Response},
            MockchainBackend, MockchainHandle,
        },
        executor::pyo3::PyO3Executor,
    };
    use rand::Rng;
    use tokio::sync::mpsc::error::TryRecvError;

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

    #[derive(Debug)]
    struct DummySink<T: NetworkBackend> {
        msg_tx: mpsc::Sender<T::Message>,
        req_tx: mpsc::Sender<Vec<u8>>,
        resp_tx: mpsc::Sender<Vec<u8>>,
    }

    impl DummySink<MockchainBackend> {
        pub fn new() -> (
            Self,
            mpsc::Receiver<<MockchainBackend as NetworkBackend>::Message>,
            mpsc::Receiver<Vec<u8>>,
            mpsc::Receiver<Vec<u8>>,
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
            payload: Vec<u8>,
        ) -> crate::Result<<MockchainBackend as NetworkBackend>::RequestId> {
            self.req_tx.send(payload).await.unwrap();
            Ok(RequestId(0u64))
        }

        async fn send_response(
            &mut self,
            request_id: <MockchainBackend as NetworkBackend>::RequestId,
            payload: Vec<u8>,
        ) -> crate::Result<()> {
            self.resp_tx.send(payload).await.unwrap();
            Ok(())
        }
    }

    #[tokio::test]
    async fn apply_connection_upgrade() {
        let mut rng = rand::thread_rng();
        let (mut overseer, _) = Overseer::<MockchainBackend, PyO3Executor<MockchainBackend>>::new();
        let interface = rng.gen();
        let peer = rng.gen();
        let (filter, filter_handle) =
            Filter::<MockchainBackend, PyO3Executor<MockchainBackend>>::new(
                interface,
                overseer.filter_event_tx.clone(),
            );
        let index = overseer.links.add_node(interface);

        overseer.interfaces.insert(
            interface,
            InterfaceInfo::<MockchainBackend>::new(
                index,
                MockchainHandle::new(
                    interface,
                    "[::1]:0".parse().unwrap(),
                    InterfaceType::Masquerade,
                )
                .await
                .unwrap()
                .0,
                filter_handle,
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
    async fn link_interfaces() {
        let mut rng = rand::thread_rng();
        let (mut overseer, _) = Overseer::<MockchainBackend, PyO3Executor<MockchainBackend>>::new();
        let interfaces = vec![rng.gen(), rng.gen(), rng.gen(), rng.gen()];
        let mut receivers = Vec::new();

        for interface in &interfaces {
            let (tx, rx) = mpsc::channel(64);
            let filter_handle = FilterHandle::new(tx);
            let index = overseer.links.add_node(*interface);

            receivers.push(rx);
            overseer.interfaces.insert(
                *interface,
                InterfaceInfo::<MockchainBackend>::new(
                    index,
                    MockchainHandle::new(
                        *interface,
                        "[::1]:0".parse().unwrap(),
                        InterfaceType::Masquerade,
                    )
                    .await
                    .unwrap()
                    .0,
                    filter_handle,
                ),
            );
        }

        // link interfaces and verify the graph is updated accordingly
        assert_eq!(overseer.links.edge_count(), 0);
        assert_eq!(overseer.links.node_count(), 4);

        assert_eq!(
            overseer.link_interfaces(interfaces[0], interfaces[1]),
            Ok(())
        );
        assert_eq!(overseer.links.edge_count(), 1);
        assert_eq!(overseer.links.node_count(), 4);

        assert_eq!(
            overseer.link_interfaces(interfaces[1], interfaces[3]),
            Ok(())
        );
        assert_eq!(overseer.links.edge_count(), 2);
        assert_eq!(overseer.links.node_count(), 4);

        // inject notification to `interface[0]` and verify it's also forwarded to `interface[1]`
        // and `interface[3]` (through link to `interface[1]`) but not to `interface[2]` as its not
        // linked to any other interface
        let peer = rng.gen();
        overseer
            .inject_notification(interfaces[0], peer, ProtocolId::Transaction, rand::random())
            .await
            .unwrap();

        assert!(std::matches!(receivers[0].try_recv(), Ok(_)));
        assert!(std::matches!(receivers[1].try_recv(), Ok(_)));
        assert!(std::matches!(receivers[3].try_recv(), Ok(_)));
        assert!(std::matches!(
            receivers[2].try_recv(),
            Err(TryRecvError::Empty)
        ));

        // remove `interface[1]` and verify that the link table is updated accordingly and that `interface[3]`
        // no longer gets the injected notification as it's not linked to `interface[0]`
        assert_eq!(
            overseer.unlink_interfaces(interfaces[0], interfaces[1]),
            Ok(())
        );
        overseer
            .inject_notification(interfaces[0], peer, ProtocolId::Transaction, rand::random())
            .await
            .unwrap();

        assert!(std::matches!(receivers[0].try_recv(), Ok(_)));
        assert!(std::matches!(
            receivers[1].try_recv(),
            Err(TryRecvError::Empty)
        ));
        assert!(std::matches!(
            receivers[3].try_recv(),
            Err(TryRecvError::Empty)
        ));
        assert!(std::matches!(
            receivers[2].try_recv(),
            Err(TryRecvError::Empty)
        ));
    }
}
