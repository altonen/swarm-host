use crate::{
    backend::NetworkBackend,
    executor::pyo3::LOG_TARGET,
    executor::{
        ExecutorEvent, FromExecutorObject, NotificationHandlingResult, RequestHandlingResult,
        ResponseHandlingResult,
    },
};

use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{PyDict, PyList},
};

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
                    .ok_or(PyErr::new::<PyTypeError, _>("Peer missing"))?;
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

// TODO: zzz
pub struct ExecutorEvents<T: NetworkBackend> {
    pub events: Vec<ExecutorEvent<T>>,
}

impl<'a, T: NetworkBackend> FromPyObject<'a> for ExecutorEvents<T>
where
    <T as NetworkBackend>::PeerId: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    <T as NetworkBackend>::Request: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    <T as NetworkBackend>::Message: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    <T as NetworkBackend>::Protocol: FromExecutorObject<ExecutorType<'a> = &'a PyAny>,
    RequestHandlingResult<T>: FromPyObject<'a>,
{
    fn extract(object: &'a PyAny) -> PyResult<Self> {
        if object.is_none() {
            return PyResult::Ok(ExecutorEvents { events: Vec::new() });
        }

        let events = object.downcast::<PyList>().unwrap();
        let mut results = Vec::new();

        for item in events.iter() {
            let dict = item.downcast::<PyDict>()?;

            if let Some(peer) = dict.get_item(&"Connect") {
                let peer = T::PeerId::from_executor_object(&peer);
                results.push(ExecutorEvent::<T>::Connect { peer });
            } else if let Some(info) = dict.get_item(&"Forward") {
                let info = info.downcast::<PyDict>()?;
                let notification =
                    T::Message::from_executor_object(&info.get_item("notification").unwrap());
                let peers = info
                    .get_item("peers")
                    .unwrap()
                    .downcast::<PyList>()?
                    .iter()
                    .map(|peer| T::PeerId::from_executor_object(&peer))
                    .collect::<Vec<T::PeerId>>();
                let protocol =
                    T::Protocol::from_executor_object(&info.get_item("protocol").unwrap());

                results.push(ExecutorEvent::Forward {
                    peers,
                    protocol,
                    notification,
                });
            }
        }

        return PyResult::Ok(ExecutorEvents { events: results });
    }
}
