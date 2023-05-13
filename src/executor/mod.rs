use crate::backend::NetworkBackend;

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
// TODO: why is this using `Vec<u8>` and not `T::Response`/`T::Request`?
#[derive(Debug, PartialEq, Eq)]
pub enum RequestHandlingResult<T: NetworkBackend> {
    /// Response does not require any action from the filter.
    DoNothing,

    /// Send request to peer.
    Request {
        /// Peer ID.
        peer: T::PeerId,

        /// Payload.
        payload: Vec<u8>,
    },

    /// Respond to one or more received requests.
    Response {
        /// Responses.
        responses: Vec<(T::PeerId, Vec<u8>)>,
    },
}

/// Response handling result.
#[derive(Debug, PartialEq, Eq)]
pub enum ResponseHandlingResult<T: NetworkBackend> {
    /// Response does not require any action from the filter.
    DoNothing,

    /// Respond to one or more received requests.
    Response {
        /// Responses.
        responses: Vec<(T::PeerId, Vec<u8>)>,

        /// Another request sent to the peer, if any.
        request: Option<(T::RequestId, Vec<u8>)>,
    },
}

/// Event received from filter after polling it.
#[derive(Debug, PartialEq, Eq)]
pub enum ExecutorEvent<T: NetworkBackend> {
    /// Connect to peer.
    Connect {
        /// Peer ID.
        peer: T::PeerId,
    },

    /// Forward notification to peer(s).
    Forward {
        /// Peer IDs.
        peers: Vec<T::PeerId>,

        /// Protocol.
        protocol: T::Protocol,

        /// Notification to forward.
        notification: T::Message,
    },

    /// Send request to peer.
    SendRequest {
        /// Peer ID.
        peer: T::PeerId,

        /// Protocol.
        protocol: T::Protocol,

        /// Request.
        payload: Vec<u8>,
    },

    /// Send response.
    SendResponse {
        /// Peer ID.
        peer: T::PeerId,

        /// Response.
        payload: Vec<u8>,
    },
}

/// Trait which allows converting types defined by the `NetworkBackend` into types that `Executor` understands.
pub trait IntoExecutorObject {
    type NativeType;
    type Context<'a>;

    /// Convert `NetworkBackend` type into something executor understands.
    fn into_executor_object(self, context: Self::Context<'_>) -> Self::NativeType;
}

/// Trait which allows converting types defined returned by the `Executor` that b`NetworkBackend` understands.
pub trait FromExecutorObject {
    type ExecutorType<'a>;

    /// Convert type received from the `Executor` into something `NetworkBackend` understands.
    fn from_executor_object(executor_type: &'_ Self::ExecutorType<'_>) -> Self;
}

pub trait Executor<T: NetworkBackend>: Send + 'static {
    /// Function that can be used to initialize interface parameters before the interface is created.
    fn initialize_interface(code: String) -> crate::Result<T::InterfaceParameters>;

    /// Create new [`Executor`].
    fn new(interface: T::InterfaceId, code: String, context: Option<String>) -> crate::Result<Self>
    where
        Self: Sized;

    /// Poll the executor and get all pending events, if any.
    fn poll(&mut self) -> crate::Result<Vec<ExecutorEvent<T>>>;

    /// Register `peer` to filter.
    fn register_peer(&mut self, peer: T::PeerId) -> crate::Result<Vec<ExecutorEvent<T>>>;

    /// Discover peer.
    fn discover_peer(&mut self, peer: T::PeerId) -> crate::Result<Vec<ExecutorEvent<T>>>;

    /// Unregister `peer` from filter.
    fn unregister_peer(&mut self, peer: T::PeerId) -> crate::Result<Vec<ExecutorEvent<T>>>;

    /// Protocol opened.
    fn protocol_opened(
        &mut self,
        peer: T::PeerId,
        protocol: T::Protocol,
    ) -> crate::Result<Vec<ExecutorEvent<T>>>;

    /// Protocol closed.
    fn protocol_closed(
        &mut self,
        peer: T::PeerId,
        protocol: T::Protocol,
    ) -> crate::Result<Vec<ExecutorEvent<T>>>;

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
        protocol: T::Protocol,
        peer: T::PeerId,
        notification: T::Message,
    ) -> crate::Result<Vec<ExecutorEvent<T>>>;

    /// Inject request to filter.
    fn inject_request(
        &mut self,
        protocol: T::Protocol,
        peer: T::PeerId,
        request: T::Request,
    ) -> crate::Result<Vec<ExecutorEvent<T>>>;

    /// Inject `response` to filter.
    fn inject_response(
        &mut self,
        protocol: T::Protocol,
        peer: T::PeerId,
        response: T::Response,
    ) -> crate::Result<Vec<ExecutorEvent<T>>>;
}
