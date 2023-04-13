use crate::{backend::NetworkBackend, error::Error};

pub mod pyo3;

/// Notification handling result.
#[derive(Debug, PartialEq, Eq)]
pub enum NotificationHandlingResult {
    /// Drop the notification and don't forward it to anyone
    Drop,

    /// Delay forwarding the received notification,
    Delay {
        /// Delay in seconds.
        delay: usize,
    },

    /// Forward notification to all connected nodes.
    Forward,
}

/// Request handling result.
pub enum RequestHandlingResult {}

/// Response handling result.
pub enum ResponseHandlingResult {}

/// Trait which allows converting types defined by the `NetworkBackend` into types that `Executor` understands.
pub trait IntoExecutorObject {
    type NativeType;
    type Context<'a>;

    /// Convert `NetworkBackend` type into something executor understands.
    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType;
}

pub trait Executor<T: NetworkBackend>: Send + 'static {
    /// Create new [`Executor`].
    fn new(interface: T::InterfaceId, code: String, context: Option<String>) -> crate::Result<Self>
    where
        Self: Sized;

    /// Register `peer` to filter.
    fn register_peer(&mut self, peer: T::PeerId) -> crate::Result<()>;

    /// Unregister `peer` from filter.
    fn unregister_peer(&mut self, peer: T::PeerId) -> crate::Result<()>;

    /// Install notification filter for `protocol`.
    fn install_notification_filter(
        &mut self,
        protocol: T::Protocol,
        code: String,
    ) -> crate::Result<()>;

    /// Install request-response filter for `protocol`.
    fn install_request_response_filter(
        &mut self,
        protocol: T::Protocol,
        code: String,
    ) -> crate::Result<()>;

    /// Inject `notification` from `peer` to filter.
    fn inject_notification(
        &mut self,
        protocol: &T::Protocol,
        peer: T::PeerId,
        notification: T::Message,
    ) -> crate::Result<NotificationHandlingResult>;

    /// Inject `notification` from `peer` to filter.
    fn inject_request(
        &mut self,
        peer: T::PeerId,
        request: T::Request,
    ) -> crate::Result<RequestHandlingResult>;

    /// Inject `response` to filter.
    fn inject_response(
        &mut self,
        peer: T::PeerId,
        request_id: T::RequestId,
        response: T::Response,
    ) -> crate::Result<ResponseHandlingResult>;
}
