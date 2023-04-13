use crate::{
    backend::{
        ConnectionUpgrade, IdableRequest, Interface, InterfaceEvent, InterfaceEventStream,
        InterfaceType, NetworkBackend, PacketSink,
    },
    executor::IntoExecutorObject,
    types::DEFAULT_CHANNEL_SIZE,
};

use futures::{channel, FutureExt, StreamExt};
use pyo3::{
    conversion::AsPyPointer,
    prelude::*,
    types::{PyBytes, PyDict},
    FromPyObject, IntoPy,
};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{instrument::WithSubscriber, Subscriber};

use sc_network::{
    config::NetworkConfiguration, Command, NodeType, PeerId as SubstratePeerId,
    ProtocolName as SubstrateProtocolName, SubstrateNetwork, SubstrateNetworkEvent,
};
use sc_network_common::{
    config::{
        NonDefaultSetConfig, NonReservedPeerMode, NotificationHandshake, ProtocolId, SetConfig,
    },
    protocol::role::{Role, Roles},
    request_responses::{IncomingRequest, ProtocolConfig},
    sync::message::BlockAnnouncesHandshake,
};

use std::{collections::HashSet, iter, net::SocketAddr, time::Duration};

// TODO: this code needs some heavy refactoring
// TODO: convert `sc-network` into a module and integrate more tightly with this code

#[cfg(test)]
mod tests;

const LOG_TARGET: &'static str = "substrate";

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ProtocolName(SubstrateProtocolName);

impl serde::Serialize for ProtocolName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&*(self.0))
    }
}

struct ProtocolNameVisitor;
use std::fmt;

use serde::de::{self, Visitor};

impl<'de> Visitor<'de> for ProtocolNameVisitor {
    type Value = ProtocolName;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(ProtocolName(SubstrateProtocolName::from(value.to_owned())))
    }
}

impl<'de> Deserialize<'de> for ProtocolName {
    fn deserialize<D>(deserializer: D) -> Result<ProtocolName, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(ProtocolNameVisitor)
    }
}

impl ProtocolName {
    fn new(protocol: SubstrateProtocolName) -> Self {
        Self(protocol)
    }
}

#[derive(Debug)]
pub struct SubstrateRequest {
    id: usize,
    payload: Vec<u8>,
}

impl IntoExecutorObject for <SubstrateBackend as NetworkBackend>::Request {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        let fields = PyDict::new(context);
        fields.set_item("id", self.id.into_py(context));
        fields.set_item("payload", self.payload.into_py(context));

        let mut request = PyDict::new(context);
        request.set_item("Request", fields).unwrap();
        request.into()
    }
}

impl IdableRequest<SubstrateBackend> for SubstrateRequest {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::RequestId {
        &self.id
    }
}

#[derive(Debug, Clone)]
pub struct SubstrateResponse {
    id: usize,
    payload: Vec<u8>,
}

impl IntoExecutorObject for <SubstrateBackend as NetworkBackend>::Response {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        let fields = PyDict::new(context);
        fields.set_item("id", self.id.into_py(context));
        fields.set_item("payload", self.payload.into_py(context));

        let mut request = PyDict::new(context);
        request.set_item("Response", fields).unwrap();
        request.into()
    }
}

#[derive(Debug)]
pub struct SubstratePacketSink {
    peer: PeerId,
    tx: mpsc::Sender<Command>,
}

impl SubstratePacketSink {
    pub fn new(peer: PeerId, tx: mpsc::Sender<Command>) -> Self {
        Self { peer, tx }
    }
}

/// Abstraction which allows `swarm-host` to send packets to peer.
#[async_trait::async_trait]
impl PacketSink<SubstrateBackend> for SubstratePacketSink {
    async fn send_packet(
        &mut self,
        protocol: Option<<SubstrateBackend as NetworkBackend>::Protocol>,
        message: &<SubstrateBackend as NetworkBackend>::Message,
    ) -> crate::Result<()> {
        self.tx
            .send(Command::SendNotification {
                peer: self.peer.0,
                protocol: protocol.expect("protocol to exist").0,
                message: message.0.clone(), // TODO: remove this clone
            })
            .await
            .expect("channel to stay open");

        Ok(())
    }

    async fn send_request(
        &mut self,
        protocol: <SubstrateBackend as NetworkBackend>::Protocol,
        request: <SubstrateBackend as NetworkBackend>::Request,
    ) -> crate::Result<<SubstrateBackend as NetworkBackend>::RequestId> {
        let (tx, rx) = oneshot::channel();

        self.tx
            .send(Command::SendRequest {
                peer: self.peer.0,
                protocol: protocol.0,
                request: request.payload,
                tx,
            })
            .await
            .expect("channel to stay open");

        Ok(rx.await.expect("channel to stay open"))
    }

    async fn send_response(
        &mut self,
        request_id: <SubstrateBackend as NetworkBackend>::RequestId,
        response: <SubstrateBackend as NetworkBackend>::Response,
    ) -> crate::Result<()> {
        self.tx
            .send(Command::SendResponse {
                peer: self.peer.0,
                request_id,
                response: response.payload,
            })
            .await
            .expect("channel to stay open");

        // TODO: return error in case sending the response failed?
        Ok(())
    }
}

pub struct InterfaceHandle {
    interface_id: usize,
}

impl InterfaceHandle {
    // TODO: pass interface id here
    // TODO: pass listen address
    pub async fn new(
        interface_type: InterfaceType,
        interface_id: usize,
    ) -> crate::Result<(Self, InterfaceEventStream<SubstrateBackend>)> {
        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let (event_tx, mut event_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let (command_tx, mut command_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        // TODO: create all protocols on substrate side
        let mut network = SubstrateNetwork::new(
            NodeType::Masquerade,
            Box::new(move |fut| {
                tokio::spawn(fut);
            }),
            event_tx,
            command_rx,
        )?;
        tokio::spawn(network.run());

        // TODO: remove this and handle all messages on `substrate` side
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = event_rx.recv() => match event.expect("channel to stay open") {
                        SubstrateNetworkEvent::PeerConnected { peer } => {
                            tx.send(InterfaceEvent::PeerConnected {
                                peer: PeerId(peer),
                                interface: interface_id,
                                protocols: Vec::new(),
                                sink: Box::new(SubstratePacketSink::new(PeerId(peer), command_tx.clone())),
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::PeerDisconnected { peer } => {
                            tx.send(InterfaceEvent::PeerDisconnected {
                                peer: PeerId(peer),
                                interface: interface_id,
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::ProtocolOpened { peer, protocol } => {
                            tx.send(InterfaceEvent::ConnectionUpgraded {
                                peer: PeerId(peer),
                                interface: interface_id,
                                upgrade: ConnectionUpgrade::ProtocolOpened {
                                    protocols: HashSet::from([ProtocolName(protocol)]),
                                }
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::ProtocolClosed { peer, protocol } => {
                            tx.send(InterfaceEvent::ConnectionUpgraded {
                                peer: PeerId(peer),
                                interface: interface_id,
                                upgrade: ConnectionUpgrade::ProtocolClosed {
                                    protocols: HashSet::from([ProtocolName(protocol)]),
                                }
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::NotificationReceived { peer, protocol, notification } => {
                            tx.send(InterfaceEvent::MessageReceived {
                                peer: PeerId(peer),
                                interface: interface_id,
                                protocol: ProtocolName(protocol),
                                message: Message(notification),
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::RequestReceived { peer, protocol, request_id, request } => {
                            tx.send(InterfaceEvent::RequestReceived {
                                peer: PeerId(peer),
                                interface: interface_id,
                                protocol: ProtocolName(protocol),
                                request: SubstrateRequest {
                                    payload: request,
                                    id: request_id,
                                }
                            })
                            .await
                            .expect("channel to stay open");
                        }
                        SubstrateNetworkEvent::ResponseReceived {peer, protocol, request_id, response } => {
                            tx.send(InterfaceEvent::ResponseReceived {
                                peer: PeerId(peer),
                                interface: interface_id,
                                protocol: ProtocolName(protocol),
                                request_id,
                                response: SubstrateResponse {
                                    id: request_id,
                                    payload: response,
                                }
                            })
                            .await
                            .expect("channel to stay open");
                        }
                    },
                }
            }
        });

        Ok((Self { interface_id }, Box::pin(ReceiverStream::new(rx))))
    }
}

impl Interface<SubstrateBackend> for InterfaceHandle {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::InterfaceId {
        &self.interface_id
    }

    /// Get handle to installed filter
    fn filter(
        &self,
        filter_name: &String,
    ) -> Option<
        Box<
            dyn Fn(
                    <SubstrateBackend as NetworkBackend>::InterfaceId,
                    <SubstrateBackend as NetworkBackend>::PeerId,
                    <SubstrateBackend as NetworkBackend>::InterfaceId,
                    <SubstrateBackend as NetworkBackend>::PeerId,
                    &<SubstrateBackend as NetworkBackend>::Message,
                ) -> bool
                + Send,
        >,
    > {
        todo!();
    }

    /// Attempt to establish connection with a remote peer.
    fn connect(&mut self, address: SocketAddr) -> crate::Result<()> {
        todo!();
    }

    /// Attempt to disconnect peer from the interface.
    fn disconnect(
        &mut self,
        peer: <SubstrateBackend as NetworkBackend>::PeerId,
    ) -> crate::Result<()> {
        todo!();
    }
}

#[derive(Debug)]
pub struct SubstrateBackend {
    next_iface_id: usize,
}

impl SubstrateBackend {
    /// Create new `SubstrateBackend`.
    pub fn new() -> Self {
        Self {
            next_iface_id: 0usize,
        }
    }

    /// Allocate ID for new interface.
    pub fn next_interface_id(&mut self) -> usize {
        let iface_id = self.next_iface_id;
        self.next_iface_id += 1;
        iface_id
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct PeerId(SubstratePeerId);

impl IntoPy<PyObject> for PeerId {
    fn into_py(self, py: Python<'_>) -> PyObject {
        self.0.to_bytes().into_py(py)
    }
}

impl IntoExecutorObject for <SubstrateBackend as NetworkBackend>::PeerId {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        self.0.to_base58().into_py(context)
    }
}

impl IntoExecutorObject for <SubstrateBackend as NetworkBackend>::Message {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        self.0.into_py(context)
    }
}

impl<'a> FromPyObject<'a> for PeerId {
    fn extract(object: &'a PyAny) -> PyResult<Self> {
        let bytes = object.extract::<&[u8]>().unwrap();

        PyResult::Ok(PeerId(
            SubstratePeerId::from_bytes(bytes).expect("valid peer id"),
        ))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, FromPyObject)]
pub struct Message(Vec<u8>);

impl IntoPy<PyObject> for Message {
    fn into_py(self, py: Python<'_>) -> PyObject {
        self.0.into_py(py)
    }
}

#[async_trait::async_trait]
impl NetworkBackend for SubstrateBackend {
    type PeerId = PeerId;
    type InterfaceId = usize;
    type RequestId = usize; // TODO: get from substrate eventually, requires refactoring
    type Protocol = ProtocolName;
    type Message = Message;
    type Request = SubstrateRequest;
    type Response = SubstrateResponse;
    type InterfaceHandle = InterfaceHandle;

    /// Create new [`SubstrateBackend`].
    fn new() -> Self {
        tracing::debug!(target: LOG_TARGET, "create new substrate backend",);

        SubstrateBackend::new()
    }

    /// Start new interface for accepting incoming connections.
    ///
    /// Return a handle which allows performing actions on the interface
    /// such as publishing messages or managing peer connections and
    /// a stream which allows reading events from interface.
    async fn spawn_interface(
        &mut self,
        address: SocketAddr,
        interface_type: InterfaceType,
    ) -> crate::Result<(Self::InterfaceHandle, InterfaceEventStream<Self>)>
    where
        Self: Sized,
    {
        tracing::debug!(
            target: LOG_TARGET,
            address = ?address,
            interface_type = ?interface_type,
            "create new substrate backend",
        );

        InterfaceHandle::new(interface_type, self.next_interface_id()).await
    }
}
