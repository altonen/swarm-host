use crate::{
    backend::NetworkBackend,
    error::Error,
    executor::{
        Executor, NotificationHandlingResult, RequestHandlingResult, ResponseHandlingResult,
    },
};

use pyo3::{
    prelude::*,
    types::{PyDict, PyList},
    FromPyPointer,
};
use rand::Rng;
use tracing::Level;

/// Logging target for the file.
const LOG_TARGET: &'static str = "executor::pyo3";

struct Context(*mut pyo3::ffi::PyObject);

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

// TODO: generate random key for the python executor so that
// there is appropriate isolation between filters
pub struct PyO3Executor {
    /// Initialize context.
    context: Context,

    /// Key of the executor for code separation.
    key: u64,

    /// Generic filter code.
    code: String,

    /// Installed notification filter, if any.
    notification_filter: Option<String>,

    /// Installed request filter, if any.
    request_filter: Option<String>,

    /// Installed response filter, if any.
    response_filter: Option<String>,
}

impl<T: NetworkBackend> Executor<T> for PyO3Executor {
    fn new(interface: T::InterfaceId, code: String, context: Option<String>) -> crate::Result<Self>
    where
        Self: Sized,
    {
        let key: u64 = rand::thread_rng().gen();

        tracing::debug!(
            target: LOG_TARGET,
            ?interface,
            ?code,
            ?context,
            ?key,
            "initialize new executor"
        );

        let context = Python::with_gil(|py| -> pyo3::PyResult<*mut pyo3::ffi::PyObject> {
            let fun = PyModule::from_code(py, &code, "", format!("module{key}").as_str())?
                .getattr("initialize_ctx")?;
            let context = fun.call1((context,))?;

            Ok(Into::<Py<PyAny>>::into(context).into_ptr())
        })?;

        Ok(Self {
            context: Context(context),
            key,
            code,
            notification_filter: None,
            request_filter: None,
            response_filter: None,
        })
    }

    /// Register `peer` to filter.
    fn register_peer(&mut self, peer: T::PeerId) {
        todo!();
    }

    /// Unregister `peer` from filter.
    fn unregister_peer(&mut self, peer: T::PeerId) {
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
