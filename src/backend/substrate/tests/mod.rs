use crate::backend::{
    substrate::{SubstrateBackend, SubstrateParameters},
    InterfaceType, NetworkBackend,
};

#[tokio::test]
async fn launch_substrate_masquerade() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let mut backend = SubstrateBackend::new(SubstrateParameters {
        genesis_hash: vec![1, 3, 3, 7],
    });

    let (_handle, _stream) = backend
        .spawn_interface(
            "127.0.0.1:8888".parse().unwrap(),
            InterfaceType::Masquerade,
            None,
        )
        .await
        .unwrap();

    // loop {
    //     tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    // }
}
