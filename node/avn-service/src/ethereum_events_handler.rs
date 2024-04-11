use crate::BlockT;
use futures::lock::Mutex;
use node_primitives::AccountId;
use pallet_eth_bridge_runtime_api::EthEventHandlerApi;
use sc_client_api::{BlockBackend, UsageProvider};
use sc_keystore::LocalKeystore;
use secp256k1::Signature;
use sp_api::ApiExt;
use sp_avn_common::{
    event_discovery::{events_helpers::discovered_eth_events_partition_factory, DiscoveredEvent},
    event_types::{
        AddedValidatorData, AvtGrowthLiftedData, Error, EthEvent, EthEventId, EventData,
        LiftedData, NftCancelListingData, NftEndBatchListingData, NftMintData, NftTransferToData,
        ValidEvents,
    },
    AVN_KEY_ID,
};
use sp_block_builder::BlockBuilder;
use sp_blockchain::HeaderBackend;
use sp_keystore::Keystore;
use std::{marker::PhantomData, time::Instant};
pub use std::{path::PathBuf, sync::Arc};
use tide::Error as TideError;
use tokio::time::Duration;
use web3::{
    types::{FilterBuilder, Log, H160, H256, U64},
    Web3,
};

use crate::{server_error, setup_web3_connection, Web3Data};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;

#[derive(Debug)]
pub enum AppError {
    ErrorParsingEventLogs,
    ErrorGettingEventLogs,
    ErrorGettingBridgeContract,
    SignatureGenerationFailed,
    MissingTransactionHash,
    MissingBlockNumber,
    ParsingError(Error),
}

pub async fn identify_events(
    web3: &Web3<web3::transports::Http>,
    start_block: u32,
    end_block: u32,
    contract_addresses: Vec<H160>,
    event_signatures: Vec<H256>,
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
        match parse_log(log) {
            Ok(discovered_event) => events.push(discovered_event),
            Err(err) => return Err(err),
        }
    }
    Ok(events)
}

fn parse_log(log: Log) -> Result<DiscoveredEvent, AppError> {
    let web3_signature = log.topics[0];
    let signature = sp_core::H256::from(web3_signature.0);

    let transaction_hash = match log.transaction_hash {
        Some(transaction_hash) => transaction_hash,
        None => return Err(AppError::MissingTransactionHash),
    };

    let event_id =
        EthEventId { signature, transaction_hash: sp_core::H256::from(transaction_hash.0) };

    let event_data = match signature_to_event_type(signature) {
        Some(event_type) => {
            let topics: Vec<Vec<u8>> = log.topics.iter().map(|t| t.0.to_vec()).collect();
            match parse_event_data(event_type, log.data.0, topics) {
                Ok(data) => data,
                Err(err) => return Err(err),
            }
        },
        None => return Err(AppError::ErrorParsingEventLogs),
    };

    let block_number = log.block_number.ok_or(AppError::MissingBlockNumber)?;

    Ok(DiscoveredEvent { event: EthEvent { event_id, event_data }, block: block_number.as_u64() })
}

fn signature_to_event_type(signature: sp_core::H256) -> Option<ValidEvents> {
    match signature {
        signature if signature == ValidEvents::AddedValidator.signature() =>
            Some(ValidEvents::AddedValidator),
        signature if signature == ValidEvents::Lifted.signature() => Some(ValidEvents::Lifted),
        signature if signature == ValidEvents::NftMint.signature() => Some(ValidEvents::NftMint),
        signature if signature == ValidEvents::NftTransferTo.signature() =>
            Some(ValidEvents::NftTransferTo),
        signature if signature == ValidEvents::NftCancelListing.signature() =>
            Some(ValidEvents::NftCancelListing),
        signature if signature == ValidEvents::NftEndBatchListing.signature() =>
            Some(ValidEvents::NftEndBatchListing),
        signature if signature == ValidEvents::AvtGrowthLifted.signature() =>
            Some(ValidEvents::AvtGrowthLifted),
        _ => None,
    }
}

fn parse_event_data(
    event_type: ValidEvents,
    data: Vec<u8>,
    topics: Vec<Vec<u8>>,
) -> Result<EventData, AppError> {
    match event_type {
        ValidEvents::AddedValidator => AddedValidatorData::parse_bytes(Some(data), topics)
            .map_err(|err| AppError::ParsingError(err.into()))
            .map(EventData::LogAddedValidator),
        ValidEvents::Lifted => LiftedData::parse_bytes(Some(data), topics)
            .map_err(|err| AppError::ParsingError(err.into()))
            .map(EventData::LogLifted),
        ValidEvents::NftMint => NftMintData::parse_bytes(Some(data), topics)
            .map_err(|err| AppError::ParsingError(err.into()))
            .map(EventData::LogNftMinted),
        ValidEvents::NftTransferTo => NftTransferToData::parse_bytes(Some(data), topics)
            .map_err(|err| AppError::ParsingError(err))
            .map(EventData::LogNftTransferTo),
        ValidEvents::NftCancelListing => NftCancelListingData::parse_bytes(Some(data), topics)
            .map_err(|err| AppError::ParsingError(err.into()))
            .map(EventData::LogNftCancelListing),
        ValidEvents::NftEndBatchListing => NftEndBatchListingData::parse_bytes(Some(data), topics)
            .map_err(|err| AppError::ParsingError(err.into()))
            .map(EventData::LogNftEndBatchListing),
        ValidEvents::AvtGrowthLifted => AvtGrowthLiftedData::parse_bytes(Some(data), topics)
            .map_err(|err| AppError::ParsingError(err.into()))
            .map(EventData::LogAvtGrowthLifted),
    }
}

pub struct EthEventHandlerConfig<Block: BlockT, ClientT>
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>,
    ClientT::Api: BlockBuilder<Block>,
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
    ClientT::Api: pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId>,
    ClientT::Api: BlockBuilder<Block>,
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

pub async fn start_eth_event_handler<Block, ClientT>(config: EthEventHandlerConfig<Block, ClientT>)
where
    Block: BlockT,
    ClientT: BlockBackend<Block>
        + UsageProvider<Block>
        + HeaderBackend<Block>
        + sp_api::ProvideRuntimeApi<Block>,
    ClientT::Api:
        pallet_eth_bridge_runtime_api::EthEventHandlerApi<Block, AccountId> + ApiExt<Block>,
    ClientT::Api: BlockBuilder<Block>,
{
    if config.initialise_web3().await.is_err() {
        return
    }

    let client = config.client;
    let mut api = client.runtime_api();

    let public_keys = config.keystore.clone().sr25519_public_keys(AVN_KEY_ID).clone();
    // TODO: find the proper way to get current collator's public key
    let current_public_key = public_keys[0];

    log::info!("⛓️  ETH EVENT HANDLER INITIALIZED");

    loop {
        let best_hash = client.info().best_hash;
        api.register_extension(
            config.offchain_transaction_pool_factory.offchain_transaction_pool(best_hash),
        );
        let (range, partition_id) = match api.query_active_block_range(best_hash) {
            Ok(result) => result,
            Err(err) => {
                log::error!("Failed to query active block range: {:?}", err);
                return
            },
        };

        let contract_address = match api.query_bridge_contract(best_hash) {
            Ok(address) => address,
            Err(err) => {
                log::error!("Failed to query bridge contract: {:?}", err);
                return
            },
        };
        let contract_address_web3 =
            web3::types::H160::from_slice(&contract_address.to_fixed_bytes());
        let contract_addresses = vec![contract_address_web3];

        let start_block = range.start_block;
        let end_block = start_block + range.length;

        let event_signatures = match api.query_signatures(best_hash) {
            Ok(event_signatures) => event_signatures,
            Err(err) => {
                log::error!("Failed to query event signatures: {:?}", err);
                return
            },
        };

        let event_signatures_web3: Vec<web3::types::H256> = event_signatures
            .iter()
            .map(|h256| web3::types::H256::from_slice(&h256.to_fixed_bytes()))
            .collect();

        if let Some(web3_data_mutex) = config.web3_data_mutex.try_lock() {
            if web3_data_mutex.web3.is_none() {
                log::error!("Web3 connection not setup")
            } else {
                let web3_ref = match web3_data_mutex.web3.as_ref() {
                    Some(ref_value) => ref_value,
                    None => {
                        log::error!("Web3 connection not set up");
                        return
                    },
                };
                let has_casted_vote = match api.query_has_author_casted_event_vote(
                    best_hash,
                    current_public_key.clone().into(),
                ) {
                    Ok(result) => result,
                    Err(err) => {
                        log::error!("Failed to check if author has casted event vote: {:?}", err);
                        false 
                    },
                };
                if has_casted_vote == false {
                    let result = identify_events(
                        &web3_ref,
                        start_block,
                        end_block,
                        contract_addresses,
                        event_signatures_web3,
                    )
                    .await;

                    match result {
                        Ok(events) => {
                            let ethereum_events_partitions =
                                discovered_eth_events_partition_factory(range, events);
                            let partition = if let Some(partition) = ethereum_events_partitions
                                .iter()
                                .find(|p| p.partition() == partition_id)
                            {
                                partition.clone()
                            } else {
                                log::error!("Partition with ID {} not found", partition_id);
                                continue
                            };
                            let proof_result = api.create_proof(
                                best_hash,
                                current_public_key.clone().into(),
                                partition.clone(),
                            );

                            let proof = match proof_result {
                                Ok(proof) => proof,
                                Err(err) => {
                                    log::error!("Failed to create proof: {:?}", err);
                                    Vec::new()
                                },
                            };

                            let signature_result = config.keystore.clone().sr25519_sign(
                                AVN_KEY_ID,
                                &current_public_key.clone(),
                                &proof.into_boxed_slice().as_ref(),
                            );

                            if let Ok(Some(sig)) = signature_result {
                                let signature = sig;

                                let author = current_public_key.clone().into();
                                let _ = api.submit_vote(
                                    best_hash,
                                    author,
                                    partition.clone(),
                                    signature.clone(),
                                );
                            }
                        },
                        Err(error) => {
                            log::error!("No discovered events: {:?}", error);
                        },
                    }
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(SLEEP_TIME)).await;
                }
            }
        } else {
            log::error!("Failed to acquire web3 data mutex.")
        }
    }
}
