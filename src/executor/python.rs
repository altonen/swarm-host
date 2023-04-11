use crate::{
    backend::NetworkBackend,
    error::Error,
    executor::{
        Executor, NotificationHandlingResult, RequestHandlingResult, ResponseHandlingResult,
    },
};

pub struct PythonExecutor {}

impl<T: NetworkBackend> Executor<T> for PythonExecutor {
    fn new() -> crate::Result<Self>
    where
        Self: Sized,
    {
        todo!();
    }

    /// Register `peer` to filter.
    fn register_peer(&mut self, peer: T::PeerId) {
        todo!();
    }

    /// Unregister `peer` from filter.
    fn unregister_peer(&mut self, peer: T::PeerId) {
        todo!();
    }

    /// Initialize filter context.
    fn initialize_filter(&mut self, code: String, context: Option<String>) -> crate::Result<()> {
        todo!();
    }

    /// Install notification filter for `protocol`.
    fn install_notification_filter(
        &mut self,
        protocol: T::Protocol,
        code: String,
    ) -> crate::Result<()> {
        todo!();
    }

    /// Install request filter for `protocol`.
    fn install_request_filter(&mut self, protocol: T::Protocol, code: String) -> crate::Result<()> {
        todo!();
    }

    /// Install response filter for `protocol`.
    fn install_response_filter(
        &mut self,
        protocol: T::Protocol,
        code: String,
    ) -> crate::Result<()> {
        todo!();
    }

    /// Inject `notification` from `peer` to filter.
    fn inject_notification(
        &mut self,
        peer: T::PeerId,
        notification: T::Message,
    ) -> crate::Result<NotificationHandlingResult> {
        todo!();
    }

    /// Inject `notification` from `peer` to filter.
    fn inject_request(
        &mut self,
        peer: T::PeerId,
        request: T::Request,
    ) -> crate::Result<RequestHandlingResult> {
        todo!();
    }

    /// Inject `response` to filter.
    fn inject_response(
        &mut self,
        peer: T::PeerId,
        request_id: T::RequestId,
        response: T::Response,
    ) -> crate::Result<ResponseHandlingResult> {
        todo!();
    }
}
