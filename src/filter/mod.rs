//! Message filtering implementation.

use crate::backend::{InterfaceType, NetworkBackend};

const LOG_TARGET: &'static str = "filter";

pub struct MessageFilter<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T: NetworkBackend> MessageFilter<T> {
    pub fn new() -> Self {
        Self {
            _marker: Default::default(),
        }
    }

    /// Register interface to [`MessageFilter`].
    pub fn register_interface(&mut self, interface: T::InterfaceId, interface_type: InterfaceType) {
        tracing::debug!(
            target: LOG_TARGET,
            interface_id = ?interface,
            interface_type = ?interface_type,
            "register interface",
        );
    }

    /// Register peer to [`MessageFilter`].
    pub fn register_peer(&mut self, peer: T::PeerId, interface: T::InterfaceId) {
        tracing::debug!(
            target: LOG_TARGET,
            peer_id = ?peer,
            interface_id = ?interface,
            "register peer",
        );
    }

    /// Inject message into [`MessageFilter`].
    ///
    /// The message is processed based on the source peer and interface IDs and message type
    /// using any user-installed filters to further alter the message processing.
    ///
    /// After the processing is done, TODO:
    pub fn inject_message(
        &mut self,
        peer: T::PeerId,
        interface: T::InterfaceId,
        message: T::Message,
    ) -> impl Iterator<Item = (T::PeerId, T::InterfaceId, T::Message)> {
        tracing::trace!(
            target: LOG_TARGET,
            peer_id = ?peer,
            interface_id = ?interface,
            message = ?message,
            "inject message",
        );

        vec![].into_iter()
    }
}
