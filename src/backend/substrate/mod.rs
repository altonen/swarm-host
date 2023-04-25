use crate::{
    backend::{
        ConnectionUpgrade, Idable, Interface, InterfaceEvent, InterfaceEventStream, InterfaceType,
        NetworkBackend, PacketSink, WithMessageInfo,
    },
    executor::{FromExecutorObject, IntoExecutorObject},
    types::DEFAULT_CHANNEL_SIZE,
};

use pyo3::{prelude::*, types::PyDict, FromPyObject, IntoPy};
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use sc_network::{
    Command, Multiaddr, NodeType, PeerId as SubstratePeerId, ProtocolName as SubstrateProtocolName,
    SubstrateNetwork, SubstrateNetworkEvent,
};

use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    hash::Hasher,
    net::SocketAddr,
};

// TODO: this code needs some heavy refactoring
// TODO: convert `sc-network` into a module and integrate more tightly with this code

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "substrate";

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ProtocolName(SubstrateProtocolName);

impl serde::Serialize for ProtocolName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
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
        fields
            .set_item("id", self.id.into_py(context))
            .expect("to succeed");
        fields
            .set_item("payload", self.payload.into_py(context))
            .expect("to succeed");

        let request = PyDict::new(context);
        request.set_item("Request", fields).unwrap();
        request.into()
    }
}

impl Idable<SubstrateBackend> for SubstrateRequest {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::RequestId {
        &self.id
    }
}

impl WithMessageInfo for SubstrateRequest {
    fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.payload);
        hasher.finish()
    }

    fn size(&self) -> usize {
        self.payload.len()
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
        let _ = fields.set_item("id", self.id.into_py(context));
        let _ = fields.set_item("payload", self.payload.into_py(context));

        let request = PyDict::new(context);
        request.set_item("Response", fields).unwrap();
        request.into()
    }
}

impl Idable<SubstrateBackend> for SubstrateResponse {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::RequestId {
        &self.id
    }
}

impl WithMessageInfo for SubstrateResponse {
    fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.payload);
        hasher.finish()
    }

    fn size(&self) -> usize {
        self.payload.len()
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
        payload: Vec<u8>,
    ) -> crate::Result<<SubstrateBackend as NetworkBackend>::RequestId> {
        let (tx, rx) = oneshot::channel();

        self.tx
            .send(Command::SendRequest {
                peer: self.peer.0,
                protocol: protocol.0,
                request: payload,
                tx,
            })
            .await
            .expect("channel to stay open");

        Ok(rx.await.expect("channel to stay open"))
    }

    async fn send_response(
        &mut self,
        request_id: <SubstrateBackend as NetworkBackend>::RequestId,
        payload: Vec<u8>,
    ) -> crate::Result<()> {
        self.tx
            .send(Command::SendResponse {
                peer: self.peer.0,
                request_id,
                response: payload,
            })
            .await
            .expect("channel to stay open");

        // TODO: return error in case sending the response failed?
        Ok(())
    }
}

pub struct InterfaceHandle {
    command_tx: mpsc::Sender<Command>,
    interface_id: usize,
}

impl InterfaceHandle {
    // TODO: pass interface id here
    // TODO: pass listen address
    pub async fn new(
        _interface_type: InterfaceType,
        interface_id: usize,
    ) -> crate::Result<(Self, InterfaceEventStream<SubstrateBackend>)> {
        let (tx, rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let (event_tx, mut event_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let (command_tx, command_rx) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

        // TODO: create all protocols on substrate side
        let network = SubstrateNetwork::new(
            NodeType::Masquerade,
            Box::new(move |fut| {
                tokio::spawn(fut);
            }),
            event_tx,
            command_rx,
        )?;
        tokio::spawn(network.run());

        // TODO: remove this and handle all messages on `substrate` side
        let cmd_tx = command_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = event_rx.recv() => match event.expect("channel to stay open") {
                        SubstrateNetworkEvent::PeerConnected { peer } => {
                            tx.send(InterfaceEvent::PeerConnected {
                                peer: PeerId(peer),
                                interface: interface_id,
                                protocols: Vec::new(),
                                sink: Box::new(SubstratePacketSink::new(PeerId(peer), cmd_tx.clone())),
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

        Ok((
            Self {
                interface_id,
                command_tx,
            },
            Box::pin(ReceiverStream::new(rx)),
        ))
    }
}

#[async_trait::async_trait]
impl Interface<SubstrateBackend> for InterfaceHandle {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::InterfaceId {
        &self.interface_id
    }

    /// Attempt to establish connection with a remote peer.
    async fn connect(
        &mut self,
        peer: <SubstrateBackend as NetworkBackend>::PeerId,
    ) -> crate::Result<()> {
        self.command_tx
            .send(Command::Connect { peer: peer.0 })
            .await
            .expect("channel to stay open");

        Ok(())
    }

    /// Attempt to disconnect peer from the interface.
    async fn disconnect(
        &mut self,
        peer: <SubstrateBackend as NetworkBackend>::PeerId,
    ) -> crate::Result<()> {
        self.command_tx
            .send(Command::Disconnect { peer: peer.0 })
            .await
            .expect("channel to stay open");
        Ok(())
    }
}

#[derive(Debug, Clone)]
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

impl serde::Serialize for PeerId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_base58())
    }
}

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

impl FromExecutorObject for <SubstrateBackend as NetworkBackend>::PeerId {
    type ExecutorType<'a> = &'a PyAny;

    fn from_executor_object<'a>(executor_type: &'a Self::ExecutorType<'a>) -> Self {
        let multiaddr = format!(
            "/p2p/{}",
            executor_type
                .extract::<String>()
                .expect("to succeed in extarcting string")
        )
        .parse::<Multiaddr>()
        .expect("valid multiaddress");

        PeerId(
            SubstratePeerId::try_from_multiaddr(&multiaddr)
                .expect("conversion to peer id from string to succeed"),
        )
    }
}

impl IntoExecutorObject for <SubstrateBackend as NetworkBackend>::Message {
    type NativeType = pyo3::PyObject;
    type Context<'a> = pyo3::marker::Python<'a>;

    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType {
        self.0.into_py(context)
    }
}

impl FromExecutorObject for <SubstrateBackend as NetworkBackend>::RequestId {
    type ExecutorType<'a> = &'a PyAny;

    fn from_executor_object(executor_type: &'_ Self::ExecutorType<'_>) -> Self {
        executor_type.extract::<usize>().unwrap()
    }
}

impl FromExecutorObject for <SubstrateBackend as NetworkBackend>::Request {
    type ExecutorType<'a> = &'a PyAny;

    fn from_executor_object(_executor_type: &'_ Self::ExecutorType<'_>) -> Self {
        todo!();
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

impl WithMessageInfo for Message {
    fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.0);
        hasher.finish()
    }

    fn size(&self) -> usize {
        self.0.len()
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
