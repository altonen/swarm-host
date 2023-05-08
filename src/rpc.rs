use crate::{backend::NetworkBackend, types::OverseerEvent};

use jsonrpsee::{
    core::Error,
    server::{RpcModule, ServerBuilder},
};
use serde::Serialize;
use tokio::sync::{mpsc::Sender, oneshot};

use std::{net::SocketAddr, time::Duration};

const LOG_TARGET: &str = "rpc";

pub async fn run_server<T>(overseer_tx: Sender<OverseerEvent<T>>, address: SocketAddr)
where
    T: NetworkBackend + 'static,
    T::InterfaceId: Serialize,
{
    tracing::debug!(
        target: LOG_TARGET,
        address = ?address,
        "starting rpc server"
    );

    let server = ServerBuilder::default()
        .max_connections(256)
        .build(address)
        .await
        .unwrap();
    let mut module = RpcModule::new(overseer_tx);

    module
        .register_async_method("create_interface", |params, ctx| async move {
            let mut params = params.sequence();
            let address = params
                .next::<String>()
                .map_err(|_| Error::Custom(String::from("RPC bind address missing")))?
                .parse::<SocketAddr>()
                .map_err(|_| Error::Custom(String::from("Invalid socket address")))?;
            let filter = params
                .next::<String>()
                .map_err(|_| Error::Custom(String::from("Filter missing")))?;
            let poll_interval = params
                .next::<u64>()
                .map(|interval| Duration::from_millis(interval))
                .map_err(|_| Error::Custom(String::from("RPC bind address missing")))?;
            let preinit: Option<String> = params.next().ok();

            tracing::debug!(
                target: LOG_TARGET,
                ?address,
                ?poll_interval,
                "create interface"
            );

            let (tx, rx) = oneshot::channel();
            match ctx
                .send(OverseerEvent::CreateInterface {
                    address,
                    filter,
                    poll_interval,
                    preinit,
                    result: tx,
                })
                .await
            {
                Ok(_) => rx
                    .await
                    .map_err(|_| Error::Custom(String::from("Essential task closed")))?
                    .map_err(|err| Error::Custom(err.to_string())),
                Err(_) => {
                    Result::<_, Error>::Err(Error::Custom(String::from("Essential task closed")))
                }
            }
        })
        .unwrap();

    module
        .register_async_method("link_interface", |params, ctx| async move {
            let mut params = params.sequence();
            let first: T::InterfaceId = params
                .next()
                .map_err(|_| Error::Custom(String::from("RPC bind address missing")))?;
            let second: T::InterfaceId = params
                .next()
                .map_err(|_| Error::Custom(String::from("RPC bind address missing")))?;

            tracing::debug!(
                target: LOG_TARGET,
                interface = ?first,
                interface = ?second,
                "link interfaces"
            );

            let (tx, rx) = oneshot::channel();
            match ctx
                .send(OverseerEvent::LinkInterface {
                    first,
                    second,
                    result: tx,
                })
                .await
            {
                Ok(_) => rx
                    .await
                    .map_err(|_| Error::Custom(String::from("Essential task closed")))?
                    .map_err(|err| Error::Custom(err.to_string())),
                Err(_) => {
                    Result::<_, Error>::Err(Error::Custom(String::from("Essential task closed")))
                }
            }
        })
        .unwrap();

    module
        .register_async_method("unlink_interface", |params, ctx| async move {
            let mut params = params.sequence();
            let first: T::InterfaceId = params
                .next()
                .map_err(|_| Error::Custom(String::from("RPC bind address missing")))?;
            let second: T::InterfaceId = params
                .next()
                .map_err(|_| Error::Custom(String::from("RPC bind address missing")))?;

            tracing::debug!(
                target: LOG_TARGET,
                interface = ?first,
                interface = ?second,
                "unlink interfaces"
            );

            let (tx, rx) = oneshot::channel();
            match ctx
                .send(OverseerEvent::UnlinkInterface {
                    first,
                    second,
                    result: tx,
                })
                .await
            {
                Ok(_) => rx
                    .await
                    .map_err(|_| Error::Custom(String::from("Essential task closed")))?
                    .map_err(|err| Error::Custom(err.to_string())),
                Err(_) => {
                    Result::<_, Error>::Err(Error::Custom(String::from("Essential task closed")))
                }
            }
        })
        .unwrap();

    module
        .register_async_method("install_notification_filter", |params, ctx| async move {
            let mut params = params.sequence();
            let interface: T::InterfaceId = params
                .next()
                .map_err(|_| Error::Custom(String::from("Interface ID missing")))?;
            let protocol: T::Protocol = params
                .next()
                .map_err(|_| Error::Custom(String::from("Protocol missing")))?;
            let filter_code: String = params
                .next()
                .map_err(|_| Error::Custom(String::from("Filter code missing")))?;
            let context: String = params
                .next()
                .map_err(|_| Error::Custom(String::from("Context missing")))?;

            tracing::debug!(
                target: LOG_TARGET,
                interface_id = ?interface,
                ?protocol,
                "install notification filter"
            );

            let (tx, rx) = oneshot::channel();
            match ctx
                .send(OverseerEvent::InstallNotificationFilter {
                    interface,
                    protocol,
                    context,
                    filter_code,
                    result: tx,
                })
                .await
            {
                Ok(_) => rx
                    .await
                    .map_err(|_| Error::Custom(String::from("Essential task closed")))?
                    .map_err(|err| Error::Custom(err.to_string())),
                Err(_) => {
                    Result::<_, Error>::Err(Error::Custom(String::from("Essential task closed")))
                }
            }
        })
        .unwrap();

    module
        .register_async_method(
            "install_request_response_filter",
            |params, ctx| async move {
                let mut params = params.sequence();
                let interface: T::InterfaceId = params
                    .next()
                    .map_err(|_| Error::Custom(String::from("Interface ID missing")))?;
                let protocol: T::Protocol = params
                    .next()
                    .map_err(|_| Error::Custom(String::from("Protocol missing")))?;
                let filter_code: String = params
                    .next()
                    .map_err(|_| Error::Custom(String::from("Filter code missing")))?;
                let context: String = params
                    .next()
                    .map_err(|_| Error::Custom(String::from("Context missing")))?;

                tracing::debug!(
                    target: LOG_TARGET,
                    interface_id = ?interface,
                    ?protocol,
                    "install request-response filter"
                );

                let (tx, rx) = oneshot::channel();
                match ctx
                    .send(OverseerEvent::InstallRequestResponseFilter {
                        interface,
                        protocol,
                        context,
                        filter_code,
                        result: tx,
                    })
                    .await
                {
                    Ok(_) => rx
                        .await
                        .map_err(|_| Error::Custom(String::from("Essential task closed")))?
                        .map_err(|err| Error::Custom(err.to_string())),
                    Err(_) => Result::<_, Error>::Err(Error::Custom(String::from(
                        "Essential task closed",
                    ))),
                }
            },
        )
        .unwrap();

    server.start(module).unwrap().stopped().await;
}
