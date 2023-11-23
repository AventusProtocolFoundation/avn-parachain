use codec::{Decode, Encode};
use futures::lock::Mutex;
use hex::FromHex;
use jsonrpc_core::ErrorCode;
use sc_keystore::LocalKeystore;
use sp_avn_common::{EthTransaction, EthQueryRequest, EthQueryResponseType, EthQueryResponse, DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER};
use sp_core::{ecdsa::Signature, hashing::keccak_256};
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, time::Instant};
use web3::{Web3, types::TransactionReceipt, transports::Http};
use sc_client_api::{client::BlockBackend, UsageProvider};
use sp_api::CallApiAt;

pub use std::{path::PathBuf, sync::Arc};

use ethereum_types::H256;
use secp256k1::{Secp256k1, SecretKey};
use tide::{http::StatusCode, Error as TideError};
pub use web3Secp256k1::SecretKey as web3SecretKey;

pub mod extrinsic_utils;
pub mod keystore_utils;
pub mod merkle_tree_utils;
pub mod summary_utils;
pub mod web3_utils;

use crate::{
    extrinsic_utils::get_latest_finalised_block, keystore_utils::*, summary_utils::*, web3_utils::*,
};

pub use crate::web3_utils::{public_key_address, secret_key_address};
use jsonrpc_core::Error as RPCError;

/// Error types for merkle tree and extrinsic utils.
#[derive(Debug)]
pub enum Error {
    DecodeError = 1,
    ResponseError = 2,
    InvalidExtrinsicInLocalDB = 3,
    ErrorGettingBlockData = 4,
    BlockDataNotFound = 5,
    BlockNotFinalised = 6,
    ErrorGeneratingRoot = 7,
    LeafDataEmpty = 8,
    EmptyLeaves = 9,
}

impl From<Error> for i32 {
    fn from(e: Error) -> i32 {
        match e {
            Error::DecodeError => 1_i32,
            Error::ResponseError => 2_i32,
            Error::InvalidExtrinsicInLocalDB => 3_i32,
            Error::ErrorGettingBlockData => 4_i32,
            Error::BlockDataNotFound => 5_i32,
            Error::BlockNotFinalised => 6_i32,
            Error::ErrorGeneratingRoot => 7_i32,
            Error::LeafDataEmpty => 8_i32,
            Error::EmptyLeaves => 9_i32,
        }
    }
}

#[derive(Clone)]
pub struct Config<
    Block: BlockT,
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block>,
> {
    pub keystore: Arc<LocalKeystore>,
    pub keystore_path: PathBuf,
    pub avn_port: Option<String>,
    pub eth_node_url: String,
    pub web3_data_mutex: Arc<Mutex<Web3Data>>,
    pub client: Arc<ClientT>,
    pub _block: PhantomData<Block>,
}

impl<Block: BlockT, ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block>>
    Config<Block, ClientT>
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
            return Err(server_error("Failed to acquire web3 data mutex.".to_string()))
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct Response {
    pub result: serde_json::Value,
    pub num_confirmations: u64,
}

trait TxQueryData {
    fn as_encodable(&self) -> Result<Vec<u8>, TideError>;
}

impl TxQueryData for TransactionReceipt {
    fn as_encodable(&self) -> Result<Vec<u8>, TideError> {
        let json_data = serde_json::to_value(self)
            .map_err(|e| server_error(format!("Error converting transaction receipt to json: {:?}", e)))?;

        let string_data = serde_json::to_string(&json_data)
            .map_err(|e| server_error(format!("Error serialising tx receipt json to string: {:?}", e)))?;

        return Ok(string_data.as_bytes().to_vec());
    }
}

impl TxQueryData for Vec<u8> {
    fn as_encodable(&self) -> Result<Vec<u8>, TideError> {
        // For Vec<u8>, the data is the Vec itself
        Ok(self.clone())
    }
}

pub fn server_error(message: String) -> TideError {
    log::error!("⛓️ 💔 avn-service {:?}", message);
    return TideError::from_str(StatusCode::InternalServerError, format!("{:?}", message))
}

pub fn hash_with_ethereum_prefix(data_to_sign: &Vec<u8>) -> [u8; 32] {
    // T1 Solidity code expects "packed" encoding of the signed message & prefix so we concatenate
    let mut prefixed_message = b"\x19Ethereum Signed Message:\n32".to_vec();
    prefixed_message.append(&mut data_to_sign.clone());
    keccak_256(&prefixed_message)
}

// TODO: Create common version of this, eg in primitives/avn-common, to share with version in
// frame/ethereum-events/src/event_parser.rs.
pub fn to_bytes32(data: String) -> Result<[u8; 32], TideError> {
    let mut data = data.to_lowercase();
    if data.starts_with("0x") {
        data = data[2..].into();
    }

    return <[u8; 32]>::from_hex(data.clone()).map_or_else(
        |e| Err(server_error(format!("Error converting {:?} to bytes32: {:?}", data, e))),
        |bytes32| Ok(bytes32),
    )
}

fn to_eth_query_response<T: TxQueryData>(
    data: &T,
    current_block_number: u64,
    data_block_number: Option<web3::types::U64>,
) -> Result<String, TideError> {
    Ok(
        hex::encode(
            EthQueryResponse {
                data: data.as_encodable().unwrap_or(vec![]).encode(),
                num_confirmations: current_block_number - data_block_number.unwrap_or(Default::default()).as_u64(),
            }.encode()
        )
    )
}

// Parses the error message and identifies if the error is related with the nonce
// https://github.com/ethereum/go-ethereum/blob/v1.10.26/core/error.go#L48
fn error_due_to_low_nonce(error: &RPCError) -> bool {
    // Expecting a ServerError with default value (-32000) when nonce used is too low:
    // Rpc(Error { code: ServerError(-32000), message: "nonce too low", data: None })
    // https://github.com/ethereum/go-ethereum/blob/v1.10.26/rpc/json.go#L109-L123
    if error.code == ErrorCode::ServerError(-32000_i64) {
        let error_msg = error.to_string();
        return error_msg.to_lowercase().find("the tx doesn't have the correct nonce").is_some() ||
            error_msg.to_lowercase().find("nonce too low").is_some()
    }
    return false
}

async fn send_tx(
    web3_data: &mut Web3Data,
    send_request: &EthTransaction,
    sender_eth_address: &Vec<u8>,
    priv_key: [u8; 32],
) -> anyhow::Result<web3::types::H256> {
    let tx = build_raw_transaction(web3_data, send_request, &sender_eth_address).await?;

    let secret_key = web3SecretKey::from_slice(&priv_key)?;
    let web3 = web3_data.get_web3_instance()?;
    let signed_tx = web3.accounts().sign_transaction(tx, &secret_key).await?;

    Ok(send_raw_transaction(web3, signed_tx.raw_transaction).await?)
}

async fn get_call_data(web3: &Web3<Http>, current_block_number: u64, tx_hash: H256) -> Result<String, TideError> {
    let maybe_tx = web3_utils::get_tx_call_data(&web3, tx_hash).await
        .map_err(|e| server_error(format!("Error getting tx call data: {:?}", e)))?;

    let response;
    match maybe_tx {
        None => {
            server_error(format!("Transaction for tx hash {:?} is empty", tx_hash));
            response = to_eth_query_response::<Vec<u8>>(&vec![], current_block_number, None)?;
        },
        Some(data) => {
            response = to_eth_query_response::<Vec<u8>>(&data.input.0.to_vec(), current_block_number, data.block_number)?;
        }
    };

    log::trace!("⛓️  avn-service: eth query response {:?}", response);
    Ok(response)
}

async fn get_tx_receipt(web3: &Web3<Http>, current_block_number: u64, tx_hash: H256) -> Result<String, TideError> {
    let maybe_receipt = web3_utils::get_tx_receipt(web3, tx_hash).await
            .map_err(|e| server_error(format!("Error getting tx receipt: {:?}", e)))?;

    match maybe_receipt {
        None => Err(server_error(format!("Transaction receipt for tx hash {:?} is empty", tx_hash))),
        Some(receipt) => {
            let response = to_eth_query_response::<TransactionReceipt>(&receipt, current_block_number, receipt.block_number)?;
            log::trace!("⛓️  avn-service: Receipt {:?}", receipt);
            Ok(response)
        }
    }
}

#[tokio::main]
async fn send_main<Block: BlockT, ClientT>(
    mut req: tide::Request<Arc<Config<Block, ClientT>>>,
) -> Result<String, TideError>
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    log::info!("⛓️  avn-service: send Request");
    let post_body = req.body_bytes().await?;
    let send_request = &EthTransaction::decode(&mut &post_body[..]).map_err(|e| {
        server_error(format!("Error decoding eth transaction data: {:?}", e))
    })?;

    if let Some(mut mutex_web3_data) = req.state().web3_data_mutex.try_lock() {
        if mutex_web3_data.web3.is_none() {
            return Err(server_error("Web3 connection not setup".to_string()))
        }
        let keystore_path = &req.state().keystore_path;

        let my_eth_address = get_eth_address_bytes_from_keystore(&keystore_path)?;
        let my_priv_key = get_priv_key(&keystore_path, &my_eth_address)?;

        let mut tx_hash =
            send_tx(&mut *mutex_web3_data, send_request, &my_eth_address, my_priv_key).await;

        if let Err(error) = &tx_hash {
            if let Some(web3::Error::Rpc(rpc_error)) = error.downcast_ref::<web3::Error>() {
                if error_due_to_low_nonce(rpc_error) {
                    log::error!(
                        "[avn-service] 💔 First send attempt to ethereum failed: {:?}",
                        error
                    );
                    let ethereum_nonce: u64 = mutex_web3_data
                        .get_nonce(&my_eth_address, true)
                        .await
                        .map_err(|e| server_error(format!("{:?}", e)))?;
                    log::error!("Attempting resend of tx with updated nonce {:?}", ethereum_nonce);

                    tx_hash =
                        send_tx(&mut *mutex_web3_data, send_request, &my_eth_address, my_priv_key)
                            .await;
                } else {
                    return Err(server_error(format!("Error sending tx to ethereum: {:?}", error)))
                }
            }
        }

        let tx_hash = tx_hash
            .map_err(|e| server_error(format!("Error sending transaction to ethereum: {:?}", e)))?;

        mutex_web3_data.increment_nonce().map_err(|e| server_error(e.to_string()))?;

        Ok(hex::encode(tx_hash))
    } else {
        Err(server_error(format!("Failed to acquire web3 mutex")))
    }
}

#[tokio::main]
async fn view_main<Block: BlockT, ClientT>(
    mut req: tide::Request<Arc<Config<Block, ClientT>>>,
) -> Result<String, TideError>
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    log::info!("⛓️  avn-service: view Request");
    let post_body = req.body_bytes().await?;
    let view_request = &EthTransaction::decode(&mut &post_body[..]).map_err(|e| {
        server_error(format!("Error decoding eth transaction data: {:?}", e))
    })?;

    if let Some(mutex_web3_data) = req.state().web3_data_mutex.try_lock() {
        if mutex_web3_data.web3.is_none() {
            return Err(server_error("Web3 connection not setup".to_string()))
        }

        let call_request = build_call_request(view_request).await?;
        let result = mutex_web3_data.web3.as_ref().unwrap()
            .eth()
            .call(call_request, None)
            .await
            .map_err(|e| server_error(format!("Error calling view method on Ethereum: {:?}", e)))?;
        log::info!("⛓️  avn-service: view request result {:?}", result);
        Ok(hex::encode(result.0))
    } else {
        Err(server_error(format!("Failed to acquire web3 mutex")))
    }
}

#[tokio::main]
async fn tx_query_main<Block: BlockT, ClientT>(
    mut req: tide::Request<Arc<Config<Block, ClientT>>>,
) -> Result<String, TideError>
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    log::info!("⛓️  avn-service: query Request.");
    let post_body = req.body_bytes().await?;

    let request = &EthTransaction::decode(&mut &post_body[..]).map_err(|e| {
        server_error(format!("Error decoding eth transaction data: {:?}", e))
    })?;

    let query_request = &EthQueryRequest::decode(&mut &request.data[..]).map_err(|e| {
        server_error(format!("Error decoding query request data: {:?}", e))
    })?;

    if let Some(mutex_web3_data) = req.state().web3_data_mutex.try_lock() {
        if mutex_web3_data.web3.is_none() {
            return Err(server_error("Web3 connection not setup".to_string()))
        }

        let web3 = mutex_web3_data.web3.as_ref().unwrap();
        let tx_hash = H256::from_slice(&to_bytes32(hex::encode(query_request.tx_hash))?);

        let current_block_number = web3_utils::get_current_block_number(&web3).await
            .map_err(|e| server_error(format!("Error getting block number: {:?}", e)))?;

        match query_request.response_type {
            EthQueryResponseType::CallData => get_call_data(&web3, current_block_number, tx_hash).await,
            EthQueryResponseType::TransactionReceipt => get_tx_receipt(&web3, current_block_number, tx_hash).await,
        }
    } else {
        Err(server_error(format!("Failed to acquire web3 mutex")))
    }
}

pub async fn start<Block: BlockT, ClientT>(config: Config<Block, ClientT>)
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    if config.initialise_web3().await.is_err() {
        return
    }

    let port = format!(
        "127.0.0.1:{}",
        &config
            .avn_port
            .clone()
            .unwrap_or_else(|| DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER.to_string())
    );

    let mut app = tide::with_state(Arc::<Config<Block, ClientT>>::from(config));

    app.at("/eth/sign/:data_to_sign").get(
        |req: tide::Request<Arc<Config<Block, ClientT>>>| async move {
            log::info!("⛓️  avn-service: sign Request");
            let secp = Secp256k1::new();
            let keystore_path = &req.state().keystore_path;

            let data_to_sign: Vec<u8> =
                hex::decode(req.param("data_to_sign")?.trim_start_matches("0x")).map_err(|e| {
                    server_error(format!("Error converting data_to_sign into hex string {:?}", e))
                })?;

            let hashed_message = hash_with_ethereum_prefix(&data_to_sign);

            log::info!(
                "⛓️  avn-service: data to sign: {:?},\n hashed data to sign: {:?}",
                hex::encode(data_to_sign),
                hex::encode(hashed_message)
            );
            let my_eth_address = get_eth_address_bytes_from_keystore(keystore_path)?;
            let my_priv_key = get_priv_key(keystore_path, &my_eth_address)?;

            let secret = SecretKey::from_slice(&my_priv_key)?;
            let message = secp256k1::Message::from_slice(&hashed_message)?;
            let signature: Signature = secp.sign_ecdsa_recoverable(&message, &secret).into();

            Ok(hex::encode(signature.encode()))
        },
    );

    app.at("/eth/send")
        .post(|req: tide::Request<Arc<Config<Block, ClientT>>>| async move {
            // Methods that require web3 must be run within the tokio runtime (#[tokio::main])
            return send_main(req)
        });

    app.at("/eth/view")
        .post(|req: tide::Request<Arc<Config<Block, ClientT>>>| async move {
            // Methods that require web3 must be run within the tokio runtime (#[tokio::main])
            return view_main(req)
        });

    app.at("/eth/query")
        .post(|req: tide::Request<Arc<Config<Block, ClientT>>>| async move {
            // Methods that require web3 must be run within the tokio runtime (#[tokio::main])
            return tx_query_main(req)
        });


    app.at("/roothash/:from_block/:to_block").get(
        |req: tide::Request<Arc<Config<Block, ClientT>>>| async move {
            log::info!("⛓️  avn-service: roothash");
            // We cannot use a number bigger than a u32, but with block times of 12 sec it would
            // take of few hundred years before we reach it.
            let from_block_number: u32 = req.param("from_block")?.parse()?;
            let to_block_number: u32 = req.param("to_block")?.parse()?;

            let extrinsics_start_time = Instant::now();

            let extrinsics =
                get_extrinsics::<Block, ClientT>(&req, from_block_number, to_block_number)?;
            let extrinsics_duration = extrinsics_start_time.elapsed();
            log::info!(
                "⏲️  get_extrinsics on block range [{:?}, {:?}] time: {:?}",
                from_block_number,
                to_block_number,
                extrinsics_duration
            );

            if extrinsics.len() > 0 {
                let root_hash_start_time = Instant::now();
                let root_hash = generate_tree_root(extrinsics)?;
                let root_hash_duration = root_hash_start_time.elapsed();
                log::info!(
                    "⏲️  generate_tree_root on block range [{:?}, {:?}] time: {:?}",
                    from_block_number,
                    to_block_number,
                    root_hash_duration
                );

                return Ok(hex::encode(root_hash))
            }

            // the tree is empty
            Ok(hex::encode([0; 32]))
        },
    );

    app.at("/latest_finalised_block").get(
        |req: tide::Request<Arc<Config<Block, ClientT>>>| async move {
            log::info!("⛓️  avn-service: latest finalised block");
            let finalised_block_number = get_latest_finalised_block(&req.state().client);
            Ok(hex::encode(finalised_block_number.encode()))
        },
    );

    app.listen(port)
        .await
        .map_err(|e| log::error!("avn-service error: {}", e))
        .unwrap_or(());
}
