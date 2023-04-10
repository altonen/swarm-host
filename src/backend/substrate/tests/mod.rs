use crate::backend::{substrate::SubstrateBackend, InterfaceType, NetworkBackend};

#[tokio::test]
async fn launch_substrate_masquerade() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut backend = SubstrateBackend::new();

    let (_handle, _stream) = backend
        .spawn_interface("127.0.0.1:8888".parse().unwrap(), InterfaceType::Masquerade)
        .await
        .unwrap();

    // loop {
    //     tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    // }
}
