use crate::{web3_utils, BlockT, ETH_FINALITY};
use futures::lock::Mutex;
use node_primitives::AccountId;
use pallet_eth_bridge_runtime_api::EthEventHandlerApi;
use sc_client_api::{BlockBackend, UsageProvider};
use sc_keystore::LocalKeystore;
use sp_api::ApiExt;
use sp_avn_common::{
    event_discovery::{
        encode_eth_event_submission_data, events_helpers::discovered_eth_events_partition_factory,
        DiscoveredEvent, EthBlockRange, EthereumEventsPartition,
    },
    event_types::{
        AddedValidatorData, AvtGrowthLiftedData, Error, EthEvent, EthEventId, EventData,
        LiftedData, NftCancelListingData, NftEndBatchListingData, NftMintData, NftTransferToData,
        ValidEvents,
    },
    AVN_KEY_ID,
};
use sp_block_builder::BlockBuilder;
use sp_blockchain::HeaderBackend;
use sp_core::{sr25519::Public, H256 as SpH256};
use sp_keystore::Keystore;
use sp_runtime::AccountId32;
use std::{collections::HashMap, marker::PhantomData, time::Instant};
pub use std::{path::PathBuf, sync::Arc};
use tide::Error as TideError;
use tokio::time::{sleep, Duration};
use web3::{
    transports::Http,
    types::{FilterBuilder, Log, H160, H256 as Web3H256, U64},
    Web3,
};

use pallet_eth_bridge::{SUBMIT_ETHEREUM_EVENTS_HASH_CONTEXT, SUBMIT_LATEST_ETH_BLOCK_CONTEXT};

use crate::{server_error, setup_web3_connection, Web3Data};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;

pub struct EventInfo {
    event_type: ValidEvents,
    parser: fn(Vec<u8>, Vec<Vec<u8>>) -> Result<EventData, AppError>,
}

pub struct EventRegistry {
    registry: HashMap<SpH256, EventInfo>,
}

impl EventRegistry {
    pub fn new() -> Self {
        let mut m = HashMap::new();
        m.insert(
            ValidEvents::AddedValidator.signature(),
            EventInfo {
                event_type: ValidEvents::AddedValidator,
                parser: |data, topics| {
                    AddedValidatorData::parse_bytes(Some(data), topics)
                        .map_err(|err| AppError::ParsingError(err.into()))
                        .map(EventData::LogAddedValidator)
                },
            },
        );
        m.insert(
            ValidEvents::Lifted.signature(),
            EventInfo {
                event_type: ValidEvents::Lifted,
                parser: |data, topics| {
                    LiftedData::parse_bytes(Some(data), topics)
                        .map_err(|err| AppError::ParsingError(err.into()))
                        .map(|data| EventData::LogLifted(data))
                },
            },
        );
        m.insert(
            ValidEvents::AvtGrowthLifted.signature(),
            EventInfo {
                event_type: ValidEvents::AvtGrowthLifted,
                parser: |data, topics| {
                    AvtGrowthLiftedData::parse_bytes(Some(data), topics)
                        .map_err(|err| AppError::ParsingError(err.into()))
                        .map(|data| EventData::LogAvtGrowthLifted(data))
                },
            },
        );
        m.insert(
            ValidEvents::NftCancelListing.signature(),
            EventInfo {
                event_type: ValidEvents::NftCancelListing,
                parser: |data, topics| {
                    NftCancelListingData::parse_bytes(Some(data), topics)
                        .map_err(|err| AppError::ParsingError(err.into()))
                        .map(|data| EventData::LogNftCancelListing(data))
                },
            },
        );
        m.insert(
            ValidEvents::NftEndBatchListing.signature(),
            EventInfo {
                event_type: ValidEvents::NftEndBatchListing,
                parser: |data, topics| {
                    NftEndBatchListingData::parse_bytes(Some(data), topics)
                        .map_err(|err| AppError::ParsingError(err.into()))
                        .map(|data| EventData::LogNftEndBatchListing(data))
                },
            },
        );
        m.insert(
            ValidEvents::NftMint.signature(),
            EventInfo {
                event_type: ValidEvents::NftMint,
                parser: |data, topics| {
                    NftMintData::parse_bytes(Some(data), topics)
                        .map_err(|err| AppError::ParsingError(err.into()))
                        .map(|data| EventData::LogNftMinted(data))
                },
            },
        );
        m.insert(
            ValidEvents::NftTransferTo.signature(),
            EventInfo {
                event_type: ValidEvents::NftTransferTo,
                parser: |data, topics| {
                    NftTransferToData::parse_bytes(Some(data), topics)
                        .map_err(|err| AppError::ParsingError(err.into()))
                        .map(|data| EventData::LogNftTransferTo(data))
                },
            },
        );
        EventRegistry { registry: m }
    }

    pub fn get_event_info(&self, signature: &SpH256) -> Option<&EventInfo> {
        self.registry.get(signature)
    }
}

#[derive(Debug)]
pub enum AppError {
    ErrorParsingEventLogs,
    ErrorGettingEventLogs,
    ErrorGettingBridgeContract,
    Web3RetryLimitReached,
    SignatureGenerationFailed,
    MissingTransactionHash,
    MissingBlockNumber,
    MissingEventSignature,
    ParsingError(Error),
    GenericError(String),
}

pub async fn identify_events(
    web3: &Web3<web3::transports::Http>,
    start_block: u32,
    end_block: u32,
    contract_addresses: Vec<H160>,
    event_signatures: Vec<Web3H256>,
    events_registry: &EventRegistry,
) -> Result<Vec<DiscoveredEvent>, AppError> {
    let filter = FilterBuilder::default()
        .address(contract_addresses)
        .topics(Some(event_signatures), None, None, None)
        .from_block(web3::types::BlockNumber::Number(U64::from(start_block)))
        .to_block(web3::types::BlockNumber::Number(U64::from(end_block)))
        .build();

    let logs_result = web3.eth().logs(filter).await;
    let logs = match logs_result {
        Ok(logs) => logs,
        Err(_) => return Err(AppError::ErrorGettingEventLogs),
    };

    let mut events = Vec::new();

    for log in logs {
        match parse_log(log, events_registry) {
            Ok(discovered_event) => events.push(discovered_event),
            Err(err) => return Err(err),
        }
    }

    Ok(events)
}

fn parse_log(log: Log, events_registry: &EventRegistry) -> Result<DiscoveredEvent, AppError> {
    if log.topics.is_empty() {
        return Err(AppError::MissingEventSignature)
    }

    let web3_signature = log.topics[0];
    let signature = SpH256::from(web3_signature.0);

    let transaction_hash = log.transaction_hash.ok_or(AppError::MissingTransactionHash)?;

    let event_id = EthEventId { signature, transaction_hash: SpH256::from(transaction_hash.0) };

    let topics: Vec<Vec<u8>> = log.topics.iter().map(|t| t.0.to_vec()).collect();
    let event_data = parse_event_data(signature, log.data.0, topics, events_registry)
        .or_else(|_| Err(AppError::ErrorParsingEventLogs))?;

    let block_number = log.block_number.ok_or(AppError::MissingBlockNumber)?;

    Ok(DiscoveredEvent { event: EthEvent { event_id, event_data }, block: block_number.as_u64() })
}

fn parse_event_data(
    signature: SpH256,
    data: Vec<u8>,
    topics: Vec<Vec<u8>>,
    events_registry: &EventRegistry,
) -> Result<EventData, AppError> {
    (events_registry
        .get_event_info(&signature)
        .ok_or(AppError::ErrorParsingEventLogs)?
        .parser)(data, topics)
}

pub struct EthEventHandlerConfig<Block: BlockT, ClientT>
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>
        + ApiExt<Block>
        + BlockBuilder<Block>,
{
    pub keystore: Arc<LocalKeystore>,
    pub keystore_path: PathBuf,
    pub avn_port: Option<String>,
    pub eth_node_url: String,
    pub web3_data_mutex: Arc<Mutex<Web3Data>>,
    pub client: Arc<ClientT>,
    pub _block: PhantomData<Block>,
    pub offchain_transaction_pool_factory: OffchainTransactionPoolFactory<Block>,
}

impl<
        Block: BlockT,
        ClientT: BlockBackend<Block>
            + UsageProvider<Block>
            + HeaderBackend<Block>
            + sp_api::ProvideRuntimeApi<Block>,
    > EthEventHandlerConfig<Block, ClientT>
where
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>
        + ApiExt<Block>
        + BlockBuilder<Block>,
{
    pub async fn initialise_web3(&self) -> Result<(), TideError> {
        if let Some(mut web3_data_mutex) = self.web3_data_mutex.try_lock() {
            if web3_data_mutex.web3.is_some() {
                log::info!(
                    "⛓️  avn-service: web3 connection has already been initialised, skipping"
                );
                return Ok(())
            }

            let web3_init_time = Instant::now();
            log::info!("⛓️  avn-service: web3 initialisation start");

            let web3 = setup_web3_connection(&self.eth_node_url);
            if web3.is_none() {
                log::error!(
                    "💔 Error creating a web3 connection. URL is not valid {:?}",
                    &self.eth_node_url
                );
                return Err(server_error("Error creating a web3 connection".to_string()))
            }

            log::info!("⏲️  web3 init task completed in: {:?}", web3_init_time.elapsed());
            web3_data_mutex.web3 = web3;
            Ok(())
        } else {
            Err(server_error("Failed to acquire web3 data mutex.".to_string()))
        }
    }
}

pub const SLEEP_TIME: u64 = 60;
pub const RETRY_LIMIT: usize = 3;
pub const RETRY_DELAY: u64 = 5;

async fn initialize_web3_with_retries<Block, ClientT>(
    config: &EthEventHandlerConfig<Block, ClientT>,
) -> Result<(), AppError>
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>
        + ApiExt<Block>
        + BlockBuilder<Block>,
{
    let mut attempts = 0;

    while attempts < RETRY_LIMIT {
        match config.initialise_web3().await {
            Ok(_) => {
                log::info!("Successfully initialized web3 connection.");
                return Ok(())
            },
            Err(e) => {
                attempts += 1;
                log::error!("Failed to initialize web3 (attempt {}): {:?}", attempts, e);
                if attempts >= RETRY_LIMIT {
                    log::error!("Reached maximum retry limit for initializing web3.");
                    return Err(AppError::Web3RetryLimitReached)
                }
                sleep(Duration::from_secs(RETRY_DELAY)).await;
            },
        }
    }

    Err(AppError::GenericError("Failed to initialize web3 after multiple attempts.".to_string()))
}

fn find_author_account_id<T>(
    author_public_keys: Result<Vec<[u8; 32]>, T>,
    keystore_public_keys: Vec<Public>,
) -> Option<Public> {
    if let Ok(account_ids) = author_public_keys {
        let signer_keys: Vec<Public> = account_ids.iter().map(|a| Public::from_raw(*a)).collect();
        for key in keystore_public_keys {
            if signer_keys.contains(&key) {
                return Some(key)
            }
        }
    }
    None
}

pub async fn start_eth_event_handler<Block, ClientT>(config: EthEventHandlerConfig<Block, ClientT>)
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>
        + ApiExt<Block>
        + BlockBuilder<Block>,
{
    if let Err(e) = initialize_web3_with_retries(&config).await {
        log::error!("Web3 initialization ultimately failed: {:?}", e);
        return
    }

    let events_registry = EventRegistry::new();

    log::info!("⛓️  ETH EVENT HANDLER INITIALIZED");

    let current_node_public_key;
    loop {
        let author_public_keys = config
            .client
            .runtime_api()
            .query_author_signing_keys(config.client.info().best_hash)
            .map_err(|e| {
                log::error!("Error querying authors: {:?}", e);
            })
            .and_then(|opt_keys| match opt_keys {
                Some(keys) => Ok(keys),
                None => Err(()),
            });

        let public_keys = config.keystore.sr25519_public_keys(AVN_KEY_ID);
        if let Some(key) = find_author_account_id(author_public_keys.clone(), public_keys.clone()) {
            current_node_public_key = key;
            break
        }
        log::error!("Author not found. Will attempt again after a while. Chain signing keys: {:?}, keystore keys: {:?}.",
            author_public_keys,
            public_keys,
        );

        sleep(Duration::from_secs(10 * SLEEP_TIME)).await;
        continue
    }

    log::info!("Current node public key set");

    loop {
        match query_runtime_and_process(&config, &current_node_public_key, &events_registry).await {
            Ok(_) => (),
            Err(e) => log::error!("{}", e),
        }

        log::info!("Sleeping");
        sleep(Duration::from_secs(SLEEP_TIME)).await;
    }
}

async fn query_runtime_and_process<Block, ClientT>(
    config: &EthEventHandlerConfig<Block, ClientT>,
    current_node_public_key: &sp_core::sr25519::Public,
    events_registry: &EventRegistry,
) -> Result<(), String>
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>
        + ApiExt<Block>
        + BlockBuilder<Block>,
{
    let result = &config
        .client
        .runtime_api()
        .query_active_block_range(config.client.info().best_hash)
        .map_err(|err| format!("Failed to query bridge contract: {:?}", err))?;

    let web3_data_mutex = config.web3_data_mutex.lock().await;
    let web3_ref = match web3_data_mutex.web3.as_ref() {
        Some(web3) => web3,
        None => return Err("Web3 connection not set up".into()),
    };

    match result {
        // A range is active, attempt processing
        Some((range, partition_id)) => {
            log::info!("Getting events for range starting at: {:?}", range.start_block);

            if web3_utils::is_eth_block_finalised(
                &web3_ref,
                get_range_end_block(range).into(),
                ETH_FINALITY,
            )
            .await?
            {
                process_events(
                    &web3_ref,
                    &config,
                    range.clone(),
                    *partition_id,
                    &current_node_public_key,
                    &events_registry,
                )
                .await?;
            }
        },
        // There is no active range, attempt initial range voting.
        None => {
            log::info!("Active range setup - Submitting latest block");
            submit_latest_ethereum_block(&config, &current_node_public_key).await?;
        },
    };

    Ok(())
}

fn get_range_end_block(range: &EthBlockRange) -> u32 {
    range.start_block + range.length
}

async fn submit_latest_ethereum_block<Block, ClientT>(
    config: &EthEventHandlerConfig<Block, ClientT>,
    current_public_key: &sp_core::sr25519::Public,
) -> Result<(), String>
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>
        + ApiExt<Block>
        + BlockBuilder<Block>,
{
    let has_casted_vote = config
        .client
        .runtime_api()
        .query_has_author_casted_vote(config.client.info().best_hash, current_public_key.0.into())
        .map_err(|err| format!("Failed to check if author has casted latest  vote: {:?}", err))?;

    log::info!("Checking if vote has been cast already. Result: {:?}", has_casted_vote);

    if !has_casted_vote {
        let web3_data_mutex = config.web3_data_mutex.lock().await;
        let web3_ref = match web3_data_mutex.web3.as_ref() {
            Some(web3) => web3,
            None => return Err("Web3 connection not set up".into()),
        };

        let latest_seen_ethereum_block = web3_utils::get_current_block_number(web3_ref)
            .await
            .map_err(|err| format!("Failed to retrieve latest ethereum block: {:?}", err))?
            as u32;

        let proof = encode_eth_event_submission_data::<AccountId, u32>(
            &SUBMIT_LATEST_ETH_BLOCK_CONTEXT,
            &(*current_public_key).into(),
            latest_seen_ethereum_block,
        );

        let signature = config
            .keystore
            .sr25519_sign(AVN_KEY_ID, current_public_key, &proof.into_boxed_slice().as_ref())
            .map_err(|err| format!("Failed to sign the proof: {:?}", err))?
            .ok_or_else(|| "Signature generation failed".to_string())?;

        let mut runtime_api = config.client.runtime_api();
        runtime_api.register_extension(
            config
                .offchain_transaction_pool_factory
                .offchain_transaction_pool(config.client.info().best_hash),
        );

        runtime_api
            .submit_latest_ethereum_block(
                config.client.info().best_hash,
                current_public_key.clone().into(),
                latest_seen_ethereum_block,
                signature,
            )
            .map_err(|err| format!("Failed to submit latest ethereum block vote: {:?}", err))?;

        log::info!("Latest ethereum block {:?} submitted to pool successfully.", latest_seen_ethereum_block);
    }
    Ok(())
}

async fn process_events<Block, ClientT>(
    web3: &Web3<Http>,
    config: &EthEventHandlerConfig<Block, ClientT>,
    range: EthBlockRange,
    partition_id: u16,
    current_public_key: &sp_core::sr25519::Public,
    events_registry: &EventRegistry,
) -> Result<(), String>
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>
        + ApiExt<Block>
        + BlockBuilder<Block>,
{
    let contract_address = config
        .client
        .runtime_api()
        .query_bridge_contract(config.client.info().best_hash)
        .map_err(|err| format!("Failed to query bridge contract: {:?}", err))?;
    let contract_address_web3 = web3::types::H160::from_slice(&contract_address.to_fixed_bytes());
    let contract_addresses = vec![contract_address_web3];

    let end_block = get_range_end_block(&range);

    let event_signatures = config
        .client
        .runtime_api()
        .query_signatures(config.client.info().best_hash)
        .map_err(|err| format!("Failed to query event signatures: {:?}", err))?;

    let event_signatures_web3: Vec<Web3H256> = event_signatures
        .iter()
        .map(|h256| Web3H256::from_slice(&h256.to_fixed_bytes()))
        .collect();

    let has_casted_vote = config
        .client
        .runtime_api()
        .query_has_author_casted_vote(config.client.info().best_hash, current_public_key.0.into())
        .map_err(|err| format!("Failed to check if author has casted event vote: {:?}", err))?;

    if !has_casted_vote {
        execute_event_processing(
            web3,
            config,
            &event_signatures_web3,
            contract_addresses,
            range.start_block,
            end_block,
            partition_id,
            current_public_key,
            range,
            events_registry,
        )
        .await
    } else {
        Ok(())
    }
}

async fn execute_event_processing<Block, ClientT>(
    web3: &Web3<Http>,
    config: &EthEventHandlerConfig<Block, ClientT>,
    event_signatures_web3: &[Web3H256],
    contract_addresses: Vec<H160>,
    start_block: u32,
    end_block: u32,
    partition_id: u16,
    current_public_key: &sp_core::sr25519::Public,
    range: EthBlockRange,
    events_registry: &EventRegistry,
) -> Result<(), String>
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>
        + ApiExt<Block>
        + BlockBuilder<Block>,
{
    let events = identify_events(
        web3,
        start_block,
        end_block,
        contract_addresses,
        event_signatures_web3.to_vec(),
        events_registry,
    )
    .await
    .map_err(|err| format!("Error retrieving events: {:?}", err))?;

    let ethereum_events_partitions = discovered_eth_events_partition_factory(range, events);
    let partition = ethereum_events_partitions
        .iter()
        .find(|p| p.partition() == partition_id)
        .ok_or_else(|| format!("Partition with ID {} not found", partition_id))?;

    let proof = encode_eth_event_submission_data::<AccountId, &EthereumEventsPartition>(
        &SUBMIT_ETHEREUM_EVENTS_HASH_CONTEXT,
        &(*current_public_key).into(),
        &partition.clone(),
    );

    let signature = config
        .keystore
        .sr25519_sign(AVN_KEY_ID, current_public_key, &proof.into_boxed_slice().as_ref())
        .map_err(|err| format!("Failed to sign the proof: {:?}", err))?
        .ok_or_else(|| "Signature generation failed".to_string())?;

    let mut runtime_api = config.client.runtime_api();
    runtime_api.register_extension(
        config
            .offchain_transaction_pool_factory
            .offchain_transaction_pool(config.client.info().best_hash),
    );

    runtime_api
        .submit_vote(
            config.client.info().best_hash,
            current_public_key.clone().into(),
            partition.clone(),
            signature,
        )
        .map_err(|err| format!("Failed to submit vote: {:?}", err))?;

    log::info!(
        "Vote for partition [{:?}, {}] submitted to pool successfully",
        partition.range(),
        partition.id()
    );
    Ok(())
}
