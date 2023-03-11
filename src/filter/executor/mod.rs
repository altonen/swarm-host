use crate::backend::NetworkBackend;

use serde::{de::DeserializeOwned, Serialize};

use std::fmt::Debug;

pub trait Executor<T: NetworkBackend> {
    type Code: Serialize + DeserializeOwned + Debug;

    /// Create new [`Executor`].
    fn new() -> Self;

    /// Install notification filter.
    // fn filter_notification(
    //     src_interface: T::InterfaceId,
    //     src_peer: T::PeerId,
    //     dst_interface: T::InterfaceId,
    //     dst_peer: T::PeerId,
    //     protocol: T::Protocol,
    // ) -> crate::Result<()>;

    // /// Ejwjwjwjw
    // fn filter_notification(
    //     src_interface: T::InterfaceId,
    //     src_peer: T::PeerId,
    //     dst_interface: T::InterfaceId,
    //     dst_peer: T::PeerId,
    //     protocol: T::Protocol,
    //     notification: T::Message,
    // ) -> crate::Result<()>;

    fn filter_request() -> crate::Result<()>;
    fn filter_response() -> crate::Result<()>;
}
