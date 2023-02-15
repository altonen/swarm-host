//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use node_template_runtime::{self, opaque::Block, RuntimeApi};
use sc_client_api::BlockBackend;
pub use sc_executor::NativeElseWasmExecutor;
use sc_service::{error::Error as ServiceError, Configuration, TaskManager};
use std::sync::Arc;

// Our native executor instance.
pub struct ExecutorDispatch;

impl sc_executor::NativeExecutionDispatch for ExecutorDispatch {
	/// Only enable the benchmarking host functions when we actually want to benchmark.
	#[cfg(feature = "runtime-benchmarks")]
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	/// Otherwise we only use the default Substrate host functions.
	#[cfg(not(feature = "runtime-benchmarks"))]
	type ExtendHostFunctions = ();

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		node_template_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		node_template_runtime::native_version()
	}
}

/// Builds a new service for a full client.
pub fn new_custom(mut config: Configuration) -> Result<TaskManager, ServiceError> {
	let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new(
		config.wasm_method,
		config.default_heap_pages,
		config.max_runtime_instances,
		config.runtime_cache_size,
	);
	let (client, _backend, _keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(&config, None, executor)?;
	let client = Arc::new(client);
	let grandpa_protocol_name = sc_finality_grandpa::protocol_standard_name(
		&client.block_hash(0).ok().flatten().expect("Genesis block exists; qed"),
		&config.chain_spec,
	);

	config
		.network
		.extra_sets
		.push(sc_finality_grandpa::grandpa_peers_set_config(grandpa_protocol_name.clone()));

	// TODO: pass fake warp sync provider
	// let select_chain = sc_consensus::LongestChain::new(backend.clone());
	// let (grandpa_block_import, grandpa_link) = sc_finality_grandpa::block_import(
	// 	client.clone(),
	// 	&(client.clone() as Arc<_>),
	// 	select_chain.clone(),
	// 	None,
	// )?;
	// let warp_sync = Arc::new(sc_finality_grandpa::warp_proof::NetworkProvider::new(
	// 	backend.clone(),
	// 	grandpa_link.shared_authority_set().clone(),
	// 	Vec::default(),
	// ));

	let network = sc_service::build_custom_network(
		&config,
		client.clone(),
		task_manager.spawn_handle(),
		None,
		None,
	)?;
	task_manager.spawn_handle().spawn(
		"test-test",
		Some("test"),
		async move { network._run().await },
	);

	Ok(task_manager)
}
