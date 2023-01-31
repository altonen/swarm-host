#![allow(unused)]

use crate::backend::{mockchain::types::Message, Interface, InterfaceEvent, NetworkBackend};

use futures::stream::Stream;

use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

mod types;

// TODO: move these to `types.rs`
/// Unique ID identifying the interface.
type InterfaceId = usize;

/// Unique ID identifying the peer.
type PeerId = u64;

pub struct MockchainHandle {
    id: InterfaceId,
}

impl MockchainHandle {
    pub fn new(id: InterfaceId) -> Self {
        Self { id }
    }
}

impl<T: NetworkBackend> Interface<T> for MockchainHandle {
    type InterfaceId = InterfaceId;

    fn id(&self) -> &T::InterfaceId {
        todo!();
    }

    fn connect(&mut self, address: SocketAddr) -> crate::Result<()> {
        todo!();
    }

    fn disconnect(&mut self, peer: T::PeerId) -> crate::Result<()> {
        todo!();
    }

    fn event_stream(&self) -> Pin<Box<dyn Future<Output = InterfaceEvent<T>>>> {
        todo!();
    }
}

impl Stream for MockchainHandle {
    type Item = InterfaceEvent<MockchainBackend>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        todo!();
    }
}

pub struct MockchainBackend {
    next_iface_id: usize,
}

impl MockchainBackend {
    pub fn new() -> Self {
        Self {
            next_iface_id: 0usize,
        }
    }
}

impl NetworkBackend for MockchainBackend {
    type PeerId = PeerId;
    type InterfaceId = InterfaceId;
    type Message = Message;
    type InterfaceHandle = MockchainHandle;

    fn spawn_interface(&mut self, address: SocketAddr) -> crate::Result<Self::InterfaceHandle> {
        todo!();
    }
}
