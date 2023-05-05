use crate::{
    backend::NetworkBackend,
    error::{Error, ExecutorError},
    executor::{
        pyo3::conversion::ExecutorEvents, Executor, ExecutorEvent, FromExecutorObject,
        IntoExecutorObject, NotificationHandlingResult, RequestHandlingResult,
        ResponseHandlingResult,
    },
};

use pyo3::{prelude::*, FromPyPointer};
use rand::Rng;
use tracing::Level;

use std::collections::HashMap;

mod conversion;
#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &str = "executor::pyo3";

/// Logging target for noisy messages.
const LOG_TARGET_MSG: &str = "executor::pyo3::msg";

struct Context(*mut pyo3::ffi::PyObject);

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

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
}

/// Helper for calling an executor function and polling the result.
macro_rules! call_executor {
    ($py:ident, $interface:expr, $code:expr, $fn_name:literal, $ctx:expr $(, $args:expr)*) => {{
        let module = format!("module{:?}", $interface);
        let function = PyModule::from_code($py, $code, "", &module)?.getattr($fn_name)?;
        function.call1(($ctx, $($args),*))?;

        let poll = PyModule::from_code($py, $code, "", &module)?.getattr("poll")?;
        poll.call1(($ctx,))?.extract::<ExecutorEvents<T>>().map(|result| result.events)
    }};
}

impl<T: NetworkBackend> Executor<T> for PyO3Executor<T>
where
    for<'a> <T as NetworkBackend>::PeerId:
        IntoExecutorObject<Context<'a> = pyo3::marker::Python<'a>, NativeType = pyo3::PyObject>,
    for<'a> <T as NetworkBackend>::Message:
        IntoExecutorObject<Context<'a> = pyo3::marker::Python<'a>, NativeType = pyo3::PyObject>,
    for<'a> <T as NetworkBackend>::Request:
        IntoExecutorObject<Context<'a> = pyo3::marker::Python<'a>, NativeType = pyo3::PyObject>,
    for<'a> <T as NetworkBackend>::Response:
        IntoExecutorObject<Context<'a> = pyo3::marker::Python<'a>, NativeType = pyo3::PyObject>,
    for<'a> <T as NetworkBackend>::InterfaceParameters:
        FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    for<'a> RequestHandlingResult<T>: pyo3::FromPyObject<'a>,
    for<'a> ResponseHandlingResult<T>: pyo3::FromPyObject<'a>,
    for<'a> ExecutorEvents<T>: pyo3::FromPyObject<'a>,
{
    /// Function that can be used to initialize interface parameters before the interface is created.
    fn initialize_interface(code: String) -> crate::Result<T::InterfaceParameters> {
        tracing::debug!(target: LOG_TARGET, "initialize interface parameters");
        tracing::trace!(target: LOG_TARGET_MSG, ?code);

        Python::with_gil(|py| -> pyo3::PyResult<T::InterfaceParameters> {
            let fun = PyModule::from_code(
                py,
                &code,
                "",
                format!("module{}", rand::thread_rng().gen::<u64>()).as_str(),
            )?
            .getattr("initialize_interface")?;

            Ok(T::InterfaceParameters::from_executor_object(&fun.call0()?))
        })
        .map_err(From::from)
    }

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
        })
    }

    /// Poll the executor.
    fn poll(&mut self) -> crate::Result<Vec<ExecutorEvent<T>>> {
        tracing::trace!(target: LOG_TARGET, "poll executor");

        Python::with_gil(|py| -> pyo3::PyResult<Vec<ExecutorEvent<T>>> {
            // get access to types that `PyO3` understands
            //
            // SAFETY: each filter has its own context and it has the same lifetime as
            // the filter itself so it is safe to convert it to a borrowed pointer.
            let ctx: &PyAny =
                unsafe { FromPyPointer::from_borrowed_ptr_or_panic(py, self.context.0) };

            call_executor!(py, self.interface, &self.code, "poll", ctx)
        })
        .map_err(From::from)
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

    /// Discover `peer`.
    fn discover_peer(&mut self, peer: T::PeerId) -> crate::Result<Vec<ExecutorEvent<T>>> {
        tracing::trace!(target: LOG_TARGET, ?peer, "discover peer");

        Python::with_gil(|py| -> pyo3::PyResult<Vec<ExecutorEvent<T>>> {
            // get access to types that `PyO3` understands
            //
            // SAFETY: each filter has its own context and it has the same lifetime as
            // the filter itself so it is safe to convert it to a borrowed pointer.
            let ctx: &PyAny =
                unsafe { FromPyPointer::from_borrowed_ptr_or_panic(py, self.context.0) };
            let peer_py = peer.into_executor_object(py);

            call_executor!(
                py,
                self.interface,
                &self.code,
                "discover_peer",
                ctx,
                peer_py
            )
        })
        .map_err(From::from)
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

        // verify that `inject_notification()` exists in the code
        Python::with_gil(|py| {
            PyModule::from_code(
                py,
                &code,
                "",
                format!("module{:?}", self.interface).as_str(),
            )?
            .getattr("inject_notification")?;

            self.notification_filters.insert(protocol, code);
            Ok(())
        })
    }

    /// Install request-response filter for `protocol`.
    fn install_request_response_filter(
        &mut self,
        protocol: T::Protocol,
        code: String,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            ?protocol,
            interface = ?self.interface,
            "install request-response filter"
        );

        // verify that `inject_request()` and `inject_response()` exist in the code
        Python::with_gil(|py| {
            let module = PyModule::from_code(
                py,
                &code,
                "",
                format!("module{:?}", self.interface).as_str(),
            )?;
            let _ = module.getattr("inject_request")?;
            let _ = module.getattr("inject_response")?;

            self.request_response_filters.insert(protocol, code);
            Ok(())
        })
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
            .getattr("inject_notification")?;

            // get access to types that `PyO3` understands
            //
            // SAFETY: each filter has its own context and it has the same lifetime as
            // the filter itself so it is safe to convert it to a borrowed pointer.
            let ctx: &PyAny =
                unsafe { FromPyPointer::from_borrowed_ptr_or_panic(py, self.context.0) };
            let peer_py = peer.into_executor_object(py);
            let notification_py = notification.into_executor_object(py);

            fun.call1((ctx, peer_py, notification_py))?.extract()
        })
        .map_err(From::from)
    }

    /// Inject `request` from `peer` to filter.
    fn inject_request(
        &mut self,
        protocol: &T::Protocol,
        peer: T::PeerId,
        request: T::Request,
    ) -> crate::Result<RequestHandlingResult<T>> {
        let request_response_code = self.request_response_filters.get(protocol);
        let request_response_code = request_response_code
            .as_ref()
            .ok_or(Error::ExecutorError(ExecutorError::FilterDoesntExist))?;

        tracing::trace!(target: LOG_TARGET, ?protocol, "inject request");

        Python::with_gil(|py| -> pyo3::PyResult<RequestHandlingResult<T>> {
            let fun = PyModule::from_code(
                py,
                request_response_code,
                "",
                format!("module{:?}", self.interface).as_str(),
            )?
            .getattr("inject_request")?;

            // get access to types that `PyO3` understands
            //
            // SAFETY: each filter has its own context and it has the same lifetime as
            // the filter itself so it is safe to convert it to a borrowed pointer.
            let ctx: &PyAny =
                unsafe { FromPyPointer::from_borrowed_ptr_or_panic(py, self.context.0) };
            let peer_py = peer.into_executor_object(py);
            let request_py = request.into_executor_object(py);

            fun.call1((ctx, peer_py, request_py))?.extract()
        })
        .map_err(From::from)
    }

    /// Inject `response` to filter.
    fn inject_response(
        &mut self,
        protocol: &T::Protocol,
        peer: T::PeerId,
        response: T::Response,
    ) -> crate::Result<ResponseHandlingResult<T>> {
        let request_response_code = self.request_response_filters.get(protocol);
        let request_response_code = request_response_code
            .as_ref()
            .ok_or(Error::ExecutorError(ExecutorError::FilterDoesntExist))?;

        tracing::trace!(target: LOG_TARGET, ?protocol, "inject response");

        Python::with_gil(|py| -> pyo3::PyResult<ResponseHandlingResult<T>> {
            let fun = PyModule::from_code(
                py,
                request_response_code,
                "",
                format!("module{:?}", self.interface).as_str(),
            )?
            .getattr("inject_response")?;

            // get access to types that `PyO3` understands
            //
            // SAFETY: each filter has its own context and it has the same lifetime as
            // the filter itself so it is safe to convert it to a borrowed pointer.
            let ctx: &PyAny =
                unsafe { FromPyPointer::from_borrowed_ptr_or_panic(py, self.context.0) };
            let peer_py = peer.into_executor_object(py);
            let response_py = response.into_executor_object(py);

            fun.call1((ctx, peer_py, response_py))?.extract()
        })
        .map_err(From::from)
    }
}
