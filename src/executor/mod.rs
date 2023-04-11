use crate::{backend::NetworkBackend, error::Error};

/// Notification handling result.
pub enum NotificationHandlingResult {}

/// Request handling result.
pub enum RequestHandlingResult {}

/// Response handling result.
pub enum ResponseHandlingResult {}

pub trait Executor<T: NetworkBackend> {
    /// Create new [`Executor`].
    fn new() -> crate::Result<Self>
    where
        Self: Sized;

    /// Register `peer` to filter.
    fn register_peer(&mut self, peer: T::PeerId);

    /// Unregister `peer` from filter.
    fn unregister_peer(&mut self, peer: T::PeerId);

    /// Initialize filter context.
    fn initialize_filter(&mut self, code: String, context: Option<String>) -> crate::Result<()>;

    /// Install notification filter for `protocol`.
    fn install_notification_filter(
        &mut self,
        protocol: T::Protocol,
        code: String,
    ) -> crate::Result<()>;

    /// Install request filter for `protocol`.
    fn install_request_filter(&mut self, protocol: T::Protocol, code: String) -> crate::Result<()>;

    /// Install response filter for `protocol`.
    fn install_response_filter(&mut self, protocol: T::Protocol, code: String)
        -> crate::Result<()>;

    /// Inject `notification` from `peer` to filter.
    fn inject_notification(
        &mut self,
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
