//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

// std
use futures::lock::Mutex;
use sp_api::ConstructRuntimeApi;
use std::{sync::Arc, time::Duration};

use cumulus_client_cli::CollatorOptions;
use runtime_common::opaque::Block;

use node_primitives::Hash;
use sc_client_api::Backend;

// Cumulus Imports
use cumulus_client_consensus_aura::{AuraConsensus, BuildAuraConsensusParams, SlotProportion};
use cumulus_client_consensus_common::{
    ParachainBlockImport as TParachainBlockImport, ParachainConsensus,
};
use cumulus_client_network::BlockAnnounceValidator;
use cumulus_client_service::{
    build_relay_chain_interface, prepare_node_config, start_collator, start_full_node,
    StartCollatorParams, StartFullNodeParams,
};
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface};

// Substrate Imports
use sc_consensus::ImportQueue;
use sc_executor::NativeElseWasmExecutor;
use sc_network::NetworkService;
use sc_network_common::service::NetworkBlock;
use sc_service::{
    config::KeystoreConfig, Configuration, PartialComponents, TFullBackend, TFullClient,
    TaskManager,
};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sp_avn_common::{DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER, EXTERNAL_SERVICE_PORT_NUMBER_KEY};
use sp_core::{offchain::OffchainStorage, Encode};
use sp_keystore::SyncCryptoStorePtr;
use substrate_prometheus_endpoint::Registry;

use crate::{avn_config::*, common::AvnRuntimeApiCollection};
use avn_service::{self, web3_utils::Web3Data};

type ParachainClient<RuntimeApi, ExecutorDispatch> =
    TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>;

type ParachainBackend = TFullBackend<Block>;

type ParachainBlockImport<RuntimeApi, ExecutorDispatch> = TParachainBlockImport<
    Block,
    Arc<ParachainClient<RuntimeApi, ExecutorDispatch>>,
    ParachainBackend,
>;

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
pub fn new_partial<RuntimeApi, ExecutorDispatch>(
    config: &Configuration,
) -> Result<
    PartialComponents<
        ParachainClient<RuntimeApi, ExecutorDispatch>,
        ParachainBackend,
        (),
        sc_consensus::DefaultImportQueue<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>,
        sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>,
        (
            ParachainBlockImport<RuntimeApi, ExecutorDispatch>,
            Option<Telemetry>,
            Option<TelemetryWorkerHandle>,
        ),
    >,
    sc_service::Error,
>
where
    RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi: AvnRuntimeApiCollection<
        StateBackend = sc_client_api::StateBackendFor<ParachainBackend, Block>,
    >,
    ExecutorDispatch: sc_executor::NativeExecutionDispatch + 'static,
{
    let telemetry = config
        .telemetry_endpoints
        .clone()
        .filter(|x| !x.is_empty())
        .map(|endpoints| -> Result<_, sc_telemetry::Error> {
            let worker = TelemetryWorker::new(16)?;
            let telemetry = worker.handle().new_telemetry(endpoints);
            Ok((worker, telemetry))
        })
        .transpose()?;

    let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new(
        config.wasm_method,
        config.default_heap_pages,
        config.max_runtime_instances,
        config.runtime_cache_size,
    );

    let (client, backend, keystore_container, task_manager) =
        sc_service::new_full_parts::<Block, RuntimeApi, _>(
            config,
            telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
            executor,
        )?;
    let client = Arc::new(client);

    let telemetry_worker_handle = telemetry.as_ref().map(|(worker, _)| worker.handle());

    let telemetry = telemetry.map(|(worker, telemetry)| {
        task_manager.spawn_handle().spawn("telemetry", None, worker.run());
        telemetry
    });

    let transaction_pool = sc_transaction_pool::BasicPool::new_full(
        config.transaction_pool.clone(),
        config.role.is_authority().into(),
        config.prometheus_registry(),
        task_manager.spawn_essential_handle(),
        client.clone(),
    );

    let block_import = ParachainBlockImport::new(client.clone(), backend.clone());

    let import_queue = build_import_queue(
        client.clone(),
        block_import.clone(),
        config,
        telemetry.as_ref().map(|telemetry| telemetry.handle()),
        &task_manager,
    )?;

    Ok(PartialComponents {
        backend,
        client,
        import_queue,
        keystore_container,
        task_manager,
        transaction_pool,
        select_chain: (),
        other: (block_import, telemetry, telemetry_worker_handle),
    })
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_node_impl<RuntimeApi, ExecutorDispatch>(
    parachain_config: Configuration,
    polkadot_config: Configuration,
    avn_cli_config: AvnCliConfiguration,
    collator_options: CollatorOptions,
    para_id: ParaId,
    hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi, ExecutorDispatch>>)>
where
    RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi: AvnRuntimeApiCollection<
        StateBackend = sc_client_api::StateBackendFor<ParachainBackend, Block>,
    >,
    ExecutorDispatch: sc_executor::NativeExecutionDispatch + 'static,
{
    let parachain_config = prepare_node_config(parachain_config);

    let params = new_partial(&parachain_config)?;
    let (block_import, mut telemetry, telemetry_worker_handle) = params.other;

    let client = params.client.clone();
    let backend = params.backend.clone();
    let mut task_manager = params.task_manager;

    let (relay_chain_interface, collator_key) = build_relay_chain_interface(
        polkadot_config,
        &parachain_config,
        telemetry_worker_handle,
        &mut task_manager,
        collator_options.clone(),
        hwbench.clone(),
    )
    .await
    .map_err(|e| match e {
        RelayChainError::ServiceError(polkadot_service::Error::Sub(x)) => x,
        s => s.to_string().into(),
    })?;

    let block_announce_validator =
        BlockAnnounceValidator::new(relay_chain_interface.clone(), para_id);

    let force_authoring = parachain_config.force_authoring;
    let validator = parachain_config.role.is_authority();
    let prometheus_registry = parachain_config.prometheus_registry().cloned();
    let transaction_pool = params.transaction_pool.clone();
    let import_queue_service = params.import_queue.service();

    let avn_port = avn_cli_config.avn_port.clone();
    let eth_node_url: String = avn_cli_config.ethereum_node_url.clone().unwrap_or_default();

    let (network, system_rpc_tx, tx_handler_controller, start_network) =
        sc_service::build_network(sc_service::BuildNetworkParams {
            config: &parachain_config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            import_queue: params.import_queue,
            block_announce_validator_builder: Some(Box::new(|_| {
                Box::new(block_announce_validator)
            })),
            warp_sync: None,
        })?;

    if parachain_config.offchain_worker.enabled {
        let port_number = avn_port
            .clone()
            .unwrap_or_else(|| DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER.to_string());
        if let Some(mut local_db) = backend.offchain_storage() {
            local_db.set(
                sp_core::offchain::STORAGE_PREFIX,
                EXTERNAL_SERVICE_PORT_NUMBER_KEY,
                &port_number.encode(),
            );
        }

        sc_service::build_offchain_workers(
            &parachain_config,
            task_manager.spawn_handle(),
            client.clone(),
            network.clone(),
        );
    }

    let rpc_builder = {
        let client = client.clone();
        let transaction_pool = transaction_pool.clone();

        Box::new(move |deny_unsafe, _| {
            let deps = crate::rpc::FullDeps {
                client: client.clone(),
                pool: transaction_pool.clone(),
                deny_unsafe,
            };

            crate::rpc::create_full(deps).map_err(Into::into)
        })
    };

    // Assigning here before `parachain_config` is borrowed
    let parachain_config_keystore = parachain_config.keystore.clone();

    sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        rpc_builder,
        client: client.clone(),
        transaction_pool: transaction_pool.clone(),
        task_manager: &mut task_manager,
        config: parachain_config,
        keystore: params.keystore_container.sync_keystore(),
        backend,
        network: network.clone(),
        system_rpc_tx,
        tx_handler_controller,
        telemetry: telemetry.as_mut(),
    })?;

    if let Some(hwbench) = hwbench {
        sc_sysinfo::print_hwbench(&hwbench);

        if let Some(ref mut telemetry) = telemetry {
            let telemetry_handle = telemetry.handle();
            task_manager.spawn_handle().spawn(
                "telemetry_hwbench",
                None,
                sc_sysinfo::initialize_hwbench_telemetry(telemetry_handle, hwbench),
            );
        }
    }

    let announce_block = {
        let network = network.clone();
        Arc::new(move |hash, data| network.announce_block(hash, data))
    };

    let relay_chain_slot_duration = Duration::from_secs(6);

    if validator {
        let keystore_path = match parachain_config_keystore {
            KeystoreConfig::Path { path, password: _ } => Ok(path.clone()),
            _ => Err("Keystore must be local"),
        }?;

        let avn_config = avn_service::Config::<Block, _> {
            keystore: params.keystore_container.local_keystore().ok_or_else(|| {
                sc_service::Error::Application(Box::from(format!("Failed to get local keystore")))
            })?,
            keystore_path,
            avn_port,
            eth_node_url,
            web3_data_mutex: Arc::new(Mutex::new(Web3Data::new())),
            client: client.clone(),
            _block: Default::default(),
        };

        task_manager.spawn_essential_handle().spawn(
            "avn-service",
            None,
            avn_service::start(avn_config),
        );
        let parachain_consensus = build_consensus(
            client.clone(),
            block_import,
            prometheus_registry.as_ref(),
            telemetry.as_ref().map(|t| t.handle()),
            &task_manager,
            relay_chain_interface.clone(),
            transaction_pool,
            network,
            params.keystore_container.sync_keystore(),
            force_authoring,
            para_id,
        )?;

        let spawner = task_manager.spawn_handle();
        let params = StartCollatorParams {
            para_id,
            block_status: client.clone(),
            announce_block,
            client: client.clone(),
            task_manager: &mut task_manager,
            relay_chain_interface,
            spawner,
            parachain_consensus,
            import_queue: import_queue_service,
            collator_key: collator_key.expect("Command line arguments do not allow this. qed"),
            relay_chain_slot_duration,
        };

        start_collator(params).await?;
    } else {
        let params = StartFullNodeParams {
            client: client.clone(),
            announce_block,
            task_manager: &mut task_manager,
            para_id,
            relay_chain_interface,
            relay_chain_slot_duration,
            import_queue: import_queue_service,
        };

        start_full_node(params)?;
    }

    start_network.start_network();

    Ok((task_manager, client))
}

/// Build the import queue for the parachain runtime.
fn build_import_queue<RuntimeApi, ExecutorDispatch>(
    client: Arc<ParachainClient<RuntimeApi, ExecutorDispatch>>,
    block_import: ParachainBlockImport<RuntimeApi, ExecutorDispatch>,
    config: &Configuration,
    telemetry: Option<TelemetryHandle>,
    task_manager: &TaskManager,
) -> Result<
    sc_consensus::DefaultImportQueue<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>,
    sc_service::Error,
>
where
    RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi: AvnRuntimeApiCollection<
        StateBackend = sc_client_api::StateBackendFor<ParachainBackend, Block>,
    >,
    ExecutorDispatch: sc_executor::NativeExecutionDispatch + 'static,
{
    let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

    cumulus_client_consensus_aura::import_queue::<
        sp_consensus_aura::sr25519::AuthorityPair,
        _,
        _,
        _,
        _,
        _,
    >(cumulus_client_consensus_aura::ImportQueueParams {
        block_import,
        client,
        create_inherent_data_providers: move |_, _| async move {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

            let slot =
				sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
					*timestamp,
					slot_duration,
				);

            Ok((slot, timestamp))
        },
        registry: config.prometheus_registry(),
        spawner: &task_manager.spawn_essential_handle(),
        telemetry,
    })
    .map_err(Into::into)
}

fn build_consensus<RuntimeApi, ExecutorDispatch>(
    client: Arc<ParachainClient<RuntimeApi, ExecutorDispatch>>,
    block_import: ParachainBlockImport<RuntimeApi, ExecutorDispatch>,
    prometheus_registry: Option<&Registry>,
    telemetry: Option<TelemetryHandle>,
    task_manager: &TaskManager,
    relay_chain_interface: Arc<dyn RelayChainInterface>,
    transaction_pool: Arc<
        sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>,
    >,
    sync_oracle: Arc<NetworkService<Block, Hash>>,
    keystore: SyncCryptoStorePtr,
    force_authoring: bool,
    para_id: ParaId,
) -> Result<Box<dyn ParachainConsensus<Block>>, sc_service::Error>
where
    RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi: AvnRuntimeApiCollection<
        StateBackend = sc_client_api::StateBackendFor<ParachainBackend, Block>,
    >,
    ExecutorDispatch: sc_executor::NativeExecutionDispatch + 'static,
{
    let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

    let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
        task_manager.spawn_handle(),
        client.clone(),
        transaction_pool,
        prometheus_registry,
        telemetry.clone(),
    );

    let params = BuildAuraConsensusParams {
        proposer_factory,
        create_inherent_data_providers: move |_, (relay_parent, validation_data)| {
            let relay_chain_interface = relay_chain_interface.clone();
            async move {
                let parachain_inherent =
                    cumulus_primitives_parachain_inherent::ParachainInherentData::create_at(
                        relay_parent,
                        &relay_chain_interface,
                        &validation_data,
                        para_id,
                    )
                    .await;
                let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

                let slot =
		sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
			*timestamp,
			slot_duration,
		);

                let parachain_inherent = parachain_inherent.ok_or_else(|| {
                    Box::<dyn std::error::Error + Send + Sync>::from(
                        "Failed to create parachain inherent",
                    )
                })?;
                Ok((slot, timestamp, parachain_inherent))
            }
        },
        block_import,
        para_client: client,
        backoff_authoring_blocks: Option::<()>::None,
        sync_oracle,
        keystore,
        force_authoring,
        slot_duration,
        // We got around 500ms for proposing
        block_proposal_slot_portion: SlotProportion::new(1f32 / 24f32),
        // And a maximum of 750ms if slots are skipped
        max_block_proposal_slot_portion: Some(SlotProportion::new(1f32 / 16f32)),
        telemetry,
    };

    Ok(AuraConsensus::build::<sp_consensus_aura::sr25519::AuthorityPair, _, _, _, _, _, _>(params))
}

/// Start a parachain node.
pub async fn start_parachain_node<RuntimeApi, ExecutorDispatch>(
    parachain_config: Configuration,
    polkadot_config: Configuration,
    avn_cli_config: AvnCliConfiguration,
    collator_options: CollatorOptions,
    para_id: ParaId,
    hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi, ExecutorDispatch>>)>
where
    RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi, ExecutorDispatch>>
        + Send
        + Sync
        + 'static,
    RuntimeApi::RuntimeApi: AvnRuntimeApiCollection<
        StateBackend = sc_client_api::StateBackendFor<ParachainBackend, Block>,
    >,
    ExecutorDispatch: sc_executor::NativeExecutionDispatch + 'static,
{
    start_node_impl(
        parachain_config,
        polkadot_config,
        avn_cli_config,
        collator_options,
        para_id,
        hwbench,
    )
    .await
}
