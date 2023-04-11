#![allow(unused)]

use crate::{
    backend::{
        ConnectionUpgrade, IdableRequest, Interface, InterfaceEvent, InterfaceType, NetworkBackend,
        PacketSink,
    },
    ensure,
    error::Error,
    executor::Executor,
    filter::{
        Filter, FilterEvent, FilterHandle, FilterType, LinkType, MessageFilter,
        RequestHandlingResult, ResponseHandlingResult,
    },
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

/// Peer-related information.
struct PeerInfo<T: NetworkBackend> {
    /// Supported protocols.
    protocols: HashSet<T::Protocol>,

    /// Sink for sending messages to peer.
    sink: Box<dyn PacketSink<T> + Send>,
}

/// Interface information.
struct InterfaceInfo<T: NetworkBackend> {
    /// Interface handle.
    handle: T::InterfaceHandle,

    /// Interface peers.
    peers: HashMap<T::PeerId, PeerInfo<T>>,

    /// Filter for the interface.
    filter: FilterHandle<T>,
}

impl<T: NetworkBackend> InterfaceInfo<T> {
    /// Create new [`InterfaceInfo`] from `T::InterfaceHandle`.
    pub fn new(handle: T::InterfaceHandle, filter: FilterHandle<T>) -> Self {
        Self {
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

    /// Message filters.
    filter: MessageFilter<T>,

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
                filter: MessageFilter::new(),
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
                    OverseerEvent::InstallNotificationFilter { interface,
                         protocol,
                         context,
                         filter_code,
                         result
                    } => {
                        tracing::debug!(
                            target: LOG_TARGET,
                            interface_id = ?interface,
                            ?protocol,
                            "install notification filter",
                        );

                        result
                            .send(
                                self.filter
                                    .install_notification_filter(interface, protocol, context, filter_code),
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
                    let (filter, filter_handle) =
                        Filter::<T, E>::new(interface_id, self.filter_event_tx.clone());

                    tracing::trace!(target: LOG_TARGET, interface = ?interface_id, "interface created");

                    self.event_streams.push(event_stream);
                    entry.insert(InterfaceInfo::new(handle, filter_handle));
                    tokio::spawn(filter.run());

                    Ok(interface_id)
                }
                Entry::Occupied(_) => Err(Error::InterfaceAlreadyExists),
            },
            Err(err) => Err(err),
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
                        sink,
                    },
                );

                // TODO: pass protocols?
                info.filter.register_peer(peer).await;
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
                info.filter
                    .inject_notification(protocol, peer, notification)
                    .await;
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
                info.filter
                    .inject_response(protocol, request_id, response)
                    .await;
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
            types::{ProtocolId, Request, Response},
            MockchainBackend, MockchainHandle,
        },
        executor::python::PythonExecutor,
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
        let (mut overseer, _) = Overseer::<MockchainBackend, PythonExecutor>::new();
        let interface = rng.gen();
        let peer = rng.gen();
        let (filter, filter_handle) = Filter::<MockchainBackend, PythonExecutor>::new(
            interface,
            overseer.filter_event_tx.clone(),
        );

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
}
