
// TODO: provide capabilities:
//        - sync mode
//          - warp sync
//          - full sync
//          - fast sync
//	      -

fn build_network() {
	let mut request_response_protocol_configs = Vec::new();

	if warp_sync.is_none() && config.network.sync_mode.is_warp() {
		return Err("Warp sync enabled, but no warp sync provider configured.".into())
	}

	if client.requires_full_sync() {
		match config.network.sync_mode {
			SyncMode::Fast { .. } => return Err("Fast sync doesn't work for archive nodes".into()),
			SyncMode::Warp => return Err("Warp sync doesn't work for archive nodes".into()),
			SyncMode::Full => {},
		}
	}

	let protocol_id = config.protocol_id();

	let block_announce_validator = if let Some(f) = block_announce_validator_builder {
		f(client.clone())
	} else {
		Box::new(DefaultBlockAnnounceValidator)
	};

	let block_request_protocol_config = {
		// Allow both outgoing and incoming requests.
		let (handler, protocol_config) = BlockRequestHandler::new(
			&protocol_id,
			config.chain_spec.fork_id(),
			client.clone(),
			config.network.default_peers_set.in_peers as usize +
				config.network.default_peers_set.out_peers as usize,
		);
		spawn_handle.spawn("block-request-handler", Some("networking"), handler.run());
		protocol_config
	};

	let state_request_protocol_config = {
		// Allow both outgoing and incoming requests.
		let (handler, protocol_config) = StateRequestHandler::new(
			&protocol_id,
			config.chain_spec.fork_id(),
			client.clone(),
			config.network.default_peers_set_num_full as usize,
		);
		spawn_handle.spawn("state-request-handler", Some("networking"), handler.run());
		protocol_config
	};

	let (warp_sync_provider, warp_sync_protocol_config) = warp_sync
		.map(|provider| {
			// Allow both outgoing and incoming requests.
			let (handler, protocol_config) = WarpSyncRequestHandler::new(
				protocol_id.clone(),
				client
					.block_hash(0u32.into())
					.ok()
					.flatten()
					.expect("Genesis block exists; qed"),
				config.chain_spec.fork_id(),
				provider.clone(),
			);
			spawn_handle.spawn("warp-sync-request-handler", Some("networking"), handler.run());
			(Some(provider), Some(protocol_config))
		})
		.unwrap_or_default();

	let light_client_request_protocol_config = {
		// Allow both outgoing and incoming requests.
		let (handler, protocol_config) = LightClientRequestHandler::new(
			&protocol_id,
			config.chain_spec.fork_id(),
			client.clone(),
		);
		spawn_handle.spawn("light-client-request-handler", Some("networking"), handler.run());
		protocol_config
	};

	let (chain_sync_network_provider, chain_sync_network_handle) = NetworkServiceProvider::new();
	let (chain_sync, chain_sync_service, block_announce_config) = ChainSync::new(
		match config.network.sync_mode {
			SyncMode::Full => sc_network_common::sync::SyncMode::Full,
			SyncMode::Fast { skip_proofs, storage_chain_mode } =>
				sc_network_common::sync::SyncMode::LightState { skip_proofs, storage_chain_mode },
			SyncMode::Warp => sc_network_common::sync::SyncMode::Warp,
		},
		client.clone(),
		protocol_id.clone(),
		&config.chain_spec.fork_id().map(ToOwned::to_owned),
		Roles::from(&config.role),
		block_announce_validator,
		config.network.max_parallel_downloads,
		warp_sync_provider,
		config.prometheus_config.as_ref().map(|config| config.registry.clone()).as_ref(),
		chain_sync_network_handle,
		import_queue.service(),
		block_request_protocol_config.name.clone(),
		state_request_protocol_config.name.clone(),
		warp_sync_protocol_config.as_ref().map(|config| config.name.clone()),
	)?;

	request_response_protocol_configs.push(config.network.ipfs_server.then(|| {
		let (handler, protocol_config) = BitswapRequestHandler::new(client.clone());
		spawn_handle.spawn("bitswap-request-handler", Some("networking"), handler.run());
		protocol_config
	}));

	let mut network_params = sc_network::config::Params {
		role: config.role.clone(),
		executor: {
			let spawn_handle = Clone::clone(&spawn_handle);
			Box::new(move |fut| {
				spawn_handle.spawn("libp2p-node", Some("networking"), fut);
			})
		},
		network_config: config.network.clone(),
		chain: client.clone(),
		protocol_id: protocol_id.clone(),
		fork_id: config.chain_spec.fork_id().map(ToOwned::to_owned),
		chain_sync: Box::new(chain_sync),
		chain_sync_service: Box::new(chain_sync_service.clone()),
		metrics_registry: config.prometheus_config.as_ref().map(|config| config.registry.clone()),
		block_announce_config,
		request_response_protocol_configs: request_response_protocol_configs
			.into_iter()
			.chain([
				Some(block_request_protocol_config),
				Some(state_request_protocol_config),
				Some(light_client_request_protocol_config),
				warp_sync_protocol_config,
			])
			.flatten()
			.collect::<Vec<_>>(),
	};

	// crate transactions protocol and add it to the list of supported protocols of `network_params`
	let transactions_handler_proto = sc_network_transactions::TransactionsHandlerPrototype::new(
		protocol_id.clone(),
		client
			.block_hash(0u32.into())
			.ok()
			.flatten()
			.expect("Genesis block exists; qed"),
		config.chain_spec.fork_id(),
	);
	network_params
		.network_config
		.extra_sets
		.insert(0, transactions_handler_proto.set_config());

	let has_bootnodes = !network_params.network_config.boot_nodes.is_empty();
	let network_mut = sc_network::NetworkWorker::new(network_params)?;
	let network = network_mut.service().clone();

	let (tx_handler, tx_handler_controller) = transactions_handler_proto.build(
		network.clone(),
		Arc::new(TransactionPoolAdapter { pool: transaction_pool, client: client.clone() }),
		config.prometheus_config.as_ref().map(|config| &config.registry),
	)?;

	spawn_handle.spawn("network-transactions-handler", Some("networking"), tx_handler.run());
	spawn_handle.spawn(
		"chain-sync-network-service-provider",
		Some("networking"),
		chain_sync_network_provider.run(network.clone()),
	);
	spawn_handle.spawn("import-queue", None, import_queue.run(Box::new(chain_sync_service)));

	let (system_rpc_tx, system_rpc_rx) = tracing_unbounded("mpsc_system_rpc", 10_000);

	let future = build_network_future(
		config.role.clone(),
		network_mut,
		client,
		system_rpc_rx,
		has_bootnodes,
		config.announce_block,
	);

	// TODO: Normally, one is supposed to pass a list of notifications protocols supported by the
	// node through the `NetworkConfiguration` struct. But because this function doesn't know in
	// advance which components, such as GrandPa or Polkadot, will be plugged on top of the
	// service, it is unfortunately not possible to do so without some deep refactoring. To bypass
	// this problem, the `NetworkService` provides a `register_notifications_protocol` method that
	// can be called even after the network has been initialized. However, we want to avoid the
	// situation where `register_notifications_protocol` is called *after* the network actually
	// connects to other peers. For this reason, we delay the process of the network future until
	// the user calls `NetworkStarter::start_network`.
	//
	// This entire hack should eventually be removed in favour of passing the list of protocols
	// through the configuration.
	//
	// See also https://github.com/paritytech/substrate/issues/6827
	let (network_start_tx, network_start_rx) = oneshot::channel();

	// The network worker is responsible for gathering all network messages and processing
	// them. This is quite a heavy task, and at the time of the writing of this comment it
	// frequently happens that this future takes several seconds or in some situations
	// even more than a minute until it has processed its entire queue. This is clearly an
	// issue, and ideally we would like to fix the network future to take as little time as
	// possible, but we also take the extra harm-prevention measure to execute the networking
	// future using `spawn_blocking`.
	spawn_handle.spawn_blocking("network-worker", Some("networking"), async move {
		if network_start_rx.await.is_err() {
			log::warn!(
				"The NetworkStart returned as part of `build_network` has been silently dropped"
			);
			// This `return` might seem unnecessary, but we don't want to make it look like
			// everything is working as normal even though the user is clearly misusing the API.
			return
		}

		future.await
	});

	Ok((network, system_rpc_tx, tx_handler_controller, NetworkStarter(network_start_tx)))
}
