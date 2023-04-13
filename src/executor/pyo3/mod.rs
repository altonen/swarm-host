use std::collections::HashMap;

use crate::{
    backend::NetworkBackend,
    error::{Error, ExecutorError},
    executor::{
        Executor, IntoExecutorObject, NotificationHandlingResult, RequestHandlingResult,
        ResponseHandlingResult,
    },
};

use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{PyDict, PyList},
    FromPyPointer,
};
use rand::Rng;
use tracing::Level;

#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &'static str = "executor::pyo3";

/// Logging target for noisy messages.
const LOG_TARGET_MSG: &'static str = "executor::pyo3::msg";

struct Context(*mut pyo3::ffi::PyObject);

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl<'a> FromPyObject<'a> for NotificationHandlingResult {
    fn extract(object: &'a PyAny) -> PyResult<Self> {
        let dict = object.downcast::<PyDict>()?;

        if dict.get_item(&"Drop").is_some() {
            return PyResult::Ok(Self::Drop);
        } else if dict.get_item("Forward").is_some() {
            return PyResult::Ok(Self::Forward);
        } else if let Some(delay) = dict.get_item(&"Delay") {
            let delay = delay.extract::<usize>()?;
            return PyResult::Ok(Self::Delay { delay });
        }

        Err(PyErr::new::<PyTypeError, _>(format!(
            "Invalid type received: `{}`",
            dict
        )))
    }
}

/// `PyO3` executor.
pub struct PyO3Executor<T: NetworkBackend> {
    /// Initialize context.
    context: Context,

    /// ID of the interface this [`Executor`] is installed to.
    interface: T::InterfaceId,

    /// Generic filter code.
    code: String,

    /// Installed notification protocol filters.
    notification_filters: HashMap<T::Protocol, String>,

    /// Installed request-response protocol filters.
    request_response_filters: HashMap<T::Protocol, String>,

    /// Installed notification filter, if any.
    notification_filter: Option<String>,

    /// Installed request filter, if any.
    request_filter: Option<String>,

    /// Installed response filter, if any.
    response_filter: Option<String>,
}

impl<T: NetworkBackend> Executor<T> for PyO3Executor<T>
where
    for<'a> <T as NetworkBackend>::PeerId:
        IntoExecutorObject<Context<'a> = pyo3::marker::Python<'a>, NativeType = pyo3::PyObject>,
    for<'a> <T as NetworkBackend>::Message:
        IntoExecutorObject<Context<'a> = pyo3::marker::Python<'a>, NativeType = pyo3::PyObject>,
{
    fn new(interface: T::InterfaceId, code: String, context: Option<String>) -> crate::Result<Self>
    where
        Self: Sized,
    {
        tracing::debug!(
            target: LOG_TARGET,
            ?interface,
            ?interface,
            "initialize new executor"
        );
        tracing::trace!(target: LOG_TARGET_MSG, ?code, ?context);

        let context = Python::with_gil(|py| -> pyo3::PyResult<*mut pyo3::ffi::PyObject> {
            let fun = PyModule::from_code(py, &code, "", format!("module{interface:?}").as_str())?
                .getattr("initialize_ctx")?;
            let context = fun.call1((context,))?;

            Ok(Into::<Py<PyAny>>::into(context).into_ptr())
        })?;

        Ok(Self {
            context: Context(context),
            interface,
            code,
            notification_filters: HashMap::new(),
            request_response_filters: HashMap::new(),
            notification_filter: None,
            request_filter: None,
            response_filter: None,
        })
    }

    /// Register `peer` to filter.
    fn register_peer(&mut self, peer: T::PeerId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, "register peer");

        Python::with_gil(|py| {
            let fun = PyModule::from_code(
                py,
                &self.code,
                "",
                format!("module{:?}", self.interface).as_str(),
            )?
            .getattr("register_peer")?;

            // get access to types that `PyO3` understands
            //
            // SAFETY: each filter has its own context and it has the same lifetime as
            // the filter itself so it is safe to convert it to a borrowed pointer.
            let ctx: &PyAny =
                unsafe { FromPyPointer::from_borrowed_ptr_or_panic(py, self.context.0) };
            let peer_py = peer.into_executor_object(py);

            fun.call1((ctx, peer_py)).map(|_| ()).map_err(From::from)
        })
    }

    /// Unregister `peer` from filter.
    fn unregister_peer(&mut self, peer: T::PeerId) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?peer, "unregister peer");

        Python::with_gil(|py| {
            let fun = PyModule::from_code(
                py,
                &self.code,
                "",
                format!("module{:?}", self.interface).as_str(),
            )?
            .getattr("unregister_peer")?;

            // get access to types that `PyO3` understands
            //
            // SAFETY: each filter has its own context and it has the same lifetime as
            // the filter itself so it is safe to convert it to a borrowed pointer.
            let ctx: &PyAny =
                unsafe { FromPyPointer::from_borrowed_ptr_or_panic(py, self.context.0) };
            let peer_py = peer.into_executor_object(py);

            fun.call1((ctx, peer_py)).map(|_| ()).map_err(From::from)
        })
    }

    /// Install notification filter for `protocol`.
    fn install_notification_filter(
        &mut self,
        protocol: T::Protocol,
        code: String,
    ) -> crate::Result<()> {
        tracing::trace!(target: LOG_TARGET, ?protocol, "install notification filter");

        // verify that `filter_notification` exists in the code and that it has the correct signature
        Python::with_gil(|py| {
            let fun = PyModule::from_code(
                py,
                &code,
                "",
                format!("module{:?}", self.interface).as_str(),
            )?
            .getattr("filter_notification")?;

            self.notification_filters.insert(protocol, code);
            Ok(())
        })
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
        protocol: &T::Protocol,
        peer: T::PeerId,
        notification: T::Message,
    ) -> crate::Result<NotificationHandlingResult> {
        let notification_filter_code = self.notification_filters.get(protocol);
        let notification_filter_code = notification_filter_code
            .as_ref()
            .ok_or(Error::ExecutorError(ExecutorError::FilterDoesntExist))?;

        tracing::event!(
            target: LOG_TARGET,
            Level::TRACE,
            ?protocol,
            "inject notification"
        );

        Python::with_gil(|py| -> pyo3::PyResult<NotificationHandlingResult> {
            let fun = PyModule::from_code(
                py,
                notification_filter_code,
                "",
                format!("module{:?}", self.interface).as_str(),
            )?
            .getattr("filter_notification")?;

            // get access to types that `PyO3` understands
            //
            // SAFETY: each filter has its own context and it has the same lifetime as
            // the filter itself so it is safe to convert it to a borrowed pointer.
            let ctx: &PyAny =
                unsafe { FromPyPointer::from_borrowed_ptr_or_panic(py, self.context.0) };
            let peer_py = peer.into_executor_object(py);
            let notification_py = notification.into_executor_object(py);

            Ok(fun.call1((ctx, peer_py, notification_py))?.extract()?)
        })
        .map_err(From::from)
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
