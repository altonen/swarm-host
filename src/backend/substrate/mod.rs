use crate::backend::{Interface, InterfaceEventStream, InterfaceType, NetworkBackend};

use sc_network::{NetworkService, NetworkWorker};
use std::net::SocketAddr;

// TODO: create `NetworkWorker`
//        - copy code from substrate?
//        - start `NetworkWorker`
//        - what to do about syncing?

// TODO: who own the `NetworkWorker` object?
// TODO: `InterfaceHandle` contains `NetworkService`?

#[derive(Debug, Copy, Clone)]
pub enum ProtocolId {}

// TODO: generic over the interface type
pub struct InterfaceHandle {}

impl InterfaceHandle {
    pub async fn new() -> Self {
        // TODO: create `NetworkWorker` here

        Self {}
    }
}

impl Interface<SubstrateBackend> for InterfaceHandle {
    fn id(&self) -> &<SubstrateBackend as NetworkBackend>::InterfaceId {
        todo!();
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

pub struct SubstrateBackend {}

impl SubstrateBackend {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl NetworkBackend for SubstrateBackend {
    type PeerId = sc_network::PeerId;
    type InterfaceId = usize;
    type ProtocolId = ProtocolId;
    type Message = ();
    type InterfaceHandle = InterfaceHandle;

    /// Create new [`SubstrateBackend`].
    fn new() -> Self {
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
        match interface_type {
            InterfaceType::Masquerade => {
                todo!("masqueraded substrate interface not implemented");
            }
            InterfaceType::NodeBacked => {
                todo!();
            }
        }
    }
}
