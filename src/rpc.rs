#![allow(unused)]

use crate::{backend::NetworkBackend, types::OverseerEvent};

use jsonrpsee::{
    core::Error,
    server::{RpcModule, ServerBuilder},
};
use serde::Serialize;
use tokio::sync::{mpsc::Sender, oneshot};

use std::net::SocketAddr;

const LOG_TARGET: &'static str = "rpc";

// TODO: convert into a struct

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

    let server = ServerBuilder::default().build(address).await.unwrap();
    let mut module = RpcModule::new((overseer_tx));

    module
        .register_async_method("create_interface", |params, ctx| async move {
            let address = params
                .sequence()
                .next::<String>()
                .map_err(|_| Error::Custom(String::from("RPC bind address missing")))?
                .parse::<SocketAddr>()
                .map_err(|_| Error::Custom(String::from("Invalid socket address")))?;

            tracing::debug!(
                target: LOG_TARGET,
                address = ?address,
                "create interface"
            );

            let (tx, rx) = oneshot::channel();
            match ctx
                .send(OverseerEvent::CreateInterface {
                    address,
                    result: tx,
                })
                .await
            {
                Ok(_) => rx
                    .await
                    .map_err(|_| Error::Custom(String::from("Essential task closed")))?
                    .map(|id| id)
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
                    .map(|id| id)
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
                    .map(|id| id)
                    .map_err(|err| Error::Custom(err.to_string())),
                Err(_) => {
                    Result::<_, Error>::Err(Error::Custom(String::from("Essential task closed")))
                }
            }
        })
        .unwrap();

    module
        .register_async_method("add_filter", |params, ctx| async move {
            let mut params = params.sequence();
            let interface: T::InterfaceId = params
                .next()
                .map_err(|_| Error::Custom(String::from("Interface ID missing")))?;
            let filter_name: String = params
                .next()
                .map_err(|_| Error::Custom(String::from("Filter name missing")))?;

            tracing::debug!(
                target: LOG_TARGET,
                interface_id = ?interface,
                filter_name = filter_name,
                "add filter"
            );

            let (tx, rx) = oneshot::channel();
            match ctx
                .send(OverseerEvent::AddFilter {
                    interface,
                    filter_name,
                    result: tx,
                })
                .await
            {
                Ok(_) => rx
                    .await
                    .map_err(|_| Error::Custom(String::from("Essential task closed")))?
                    .map(|id| id)
                    .map_err(|err| Error::Custom(err.to_string())),
                Err(_) => {
                    Result::<_, Error>::Err(Error::Custom(String::from("Essential task closed")))
                }
            }
        })
        .unwrap();

    server.start(module).unwrap().stopped().await;
}
