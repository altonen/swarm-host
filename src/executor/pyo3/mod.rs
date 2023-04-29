use std::collections::HashMap;

use crate::{
    backend::NetworkBackend,
    error::{Error, ExecutorError},
    executor::{
        Executor, FromExecutorObject, IntoExecutorObject, NotificationHandlingResult,
        RequestHandlingResult, ResponseHandlingResult,
    },
};

use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{PyDict, PyList},
    FromPyPointer,
};
use tracing::Level;

#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &str = "executor::pyo3";

/// Logging target for noisy messages.
const LOG_TARGET_MSG: &str = "executor::pyo3::msg";

struct Context(*mut pyo3::ffi::PyObject);

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

/// Type conversion from Python dictionary to `NotificationHandlingResult`
impl<'a> FromPyObject<'a> for NotificationHandlingResult {
    fn extract(object: &'a PyAny) -> PyResult<Self> {
        let dict = object.downcast::<PyDict>()?;

        if dict.get_item("Drop").is_some() {
            return PyResult::Ok(Self::Drop);
        } else if dict.get_item("Forward").is_some() {
            return PyResult::Ok(Self::Forward);
        } else if let Some(delay) = dict.get_item("Delay") {
            let delay = delay.extract::<usize>()?;
            return PyResult::Ok(Self::Delay { delay });
        }

        Err(PyErr::new::<PyTypeError, _>(format!(
            "Invalid type received: `{}`",
            dict
        )))
    }
}

/// Type conversion from Python dictionary to `RequestHandlingResult`
impl<'a, T: NetworkBackend> FromPyObject<'a> for RequestHandlingResult<T>
where
    <T as NetworkBackend>::PeerId: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    <T as NetworkBackend>::RequestId: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    <T as NetworkBackend>::Request: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
{
    fn extract(object: &'a PyAny) -> PyResult<Self> {
        let dict = object.downcast::<PyDict>()?;

        if dict.get_item("DoNothing").is_some() {
            return PyResult::Ok(Self::DoNothing);
        } else if let Some(request) = dict.get_item("Request") {
            let dict = request.downcast::<PyDict>()?;
            let peer = dict
                .get_item("peer")
                .ok_or(PyErr::new::<PyTypeError, _>("Peer ID missing"))?;
            let payload = dict
                .get_item("payload")
                .ok_or(PyErr::new::<PyTypeError, _>("Request missing"))?
                .extract::<Vec<u8>>()?;

            return PyResult::Ok(Self::Request {
                peer: T::PeerId::from_executor_object(&peer),
                payload,
            });
        } else if let Some(result) = dict.get_item("Response") {
            let results = result.downcast::<PyList>()?;
            let mut responses = Vec::new();

            for dict in results {
                let dict = dict.downcast::<PyDict>()?;
                let peer = dict
                    .get_item("peer")
                    .ok_or(PyErr::new::<PyTypeError, _>("Peer missing"))?;
                let payload = dict
                    .get_item("payload")
                    .ok_or(PyErr::new::<PyTypeError, _>("Response missing"))?
                    .extract::<Vec<u8>>()?;

                responses.push((T::PeerId::from_executor_object(&peer), payload));
            }

            return PyResult::Ok(Self::Response { responses });
        }

        Err(PyErr::new::<PyTypeError, _>(format!(
            "Invalid type received: `{}`",
            dict
        )))
    }
}

/// Type conversion from Python dictionary to `ResponseHandlingResult`
impl<'a, T: NetworkBackend> FromPyObject<'a> for ResponseHandlingResult<T>
where
    <T as NetworkBackend>::PeerId: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    <T as NetworkBackend>::Request: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    RequestHandlingResult<T>: FromPyObject<'a>,
{
    fn extract(object: &'a PyAny) -> PyResult<Self> {
        let dict = object.downcast::<PyDict>()?;

        if dict.get_item("DoNothing").is_some() {
            return PyResult::Ok(Self::DoNothing);
        } else if let Some(result) = dict.get_item("Response") {
            let results = result.downcast::<PyDict>()?;

            let responses = results
                .get_item("Responses")
                .ok_or(PyErr::new::<PyTypeError, _>("Responses missing"))?
                .downcast::<PyList>()?;

            // TODO: handle cached requests
            // let request = results
            //     .get_item("Request")
            //     .ok_or(PyErr::new::<PyTypeError, _>("Request missing"))?;
            // let request = match RequestHandlingResult::<T>::extract(request)? {
            //     RequestHandlingResult::DoNothing {.. } | RequestHandlingResult::Response { responses }
            // }

            let mut return_results = Vec::new();
            for dict in responses {
                let dict = dict.downcast::<PyDict>()?;
                tracing::info!(target: LOG_TARGET, "{dict:#?}");
                let peer = dict
                    .get_item("peer")
                    .ok_or(PyErr::new::<PyTypeError, _>("Peer 111 missing"))?;
                let payload = dict
                    .get_item("payload")
                    .ok_or(PyErr::new::<PyTypeError, _>("Request missing"))?
                    .extract::<Vec<u8>>()?;
                return_results.push((T::PeerId::from_executor_object(&peer), payload));
            }

            return PyResult::Ok(Self::Response {
                responses: return_results,
                request: None,
            });
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
    for<'a> RequestHandlingResult<T>: pyo3::FromPyObject<'a>,
    for<'a> ResponseHandlingResult<T>: pyo3::FromPyObject<'a>,
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

        // verify that `filter_notification()` exists in the code
        Python::with_gil(|py| {
            PyModule::from_code(
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

    /// Install request-response filter for `protocol`.
    fn install_request_response_filter(
        &mut self,
        protocol: T::Protocol,
        code: String,
    ) -> crate::Result<()> {
        tracing::trace!(
            target: LOG_TARGET,
            ?protocol,
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
            let _ = module.getattr("filter_request")?;
            let _ = module.getattr("filter_response")?;

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
            .getattr("filter_notification")?;

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
            .getattr("filter_request")?;

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
            .getattr("filter_response")?;

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
