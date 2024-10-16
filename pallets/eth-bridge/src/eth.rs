use super::*;
use crate::{
    util::{try_process_query_result, unbound_params},
    Author, Config, AVN,
};
use ethabi::{Address, Function, Int, Param, ParamType, Token};
use pallet_avn::AccountToBytesConverter;
use sp_avn_common::{EthQueryRequest, EthQueryResponseType, EthTransaction};
use sp_core::{ecdsa, Get, H256};
use sp_runtime::DispatchError;
use sp_std::vec;

pub const UINT256: &[u8] = b"uint256";
pub const UINT128: &[u8] = b"uint128";
pub const UINT32: &[u8] = b"uint32";
pub const BYTES: &[u8] = b"bytes";
pub const BYTES32: &[u8] = b"bytes32";
pub const ADDRESS: &[u8] = b"address";

pub fn sign_msg_hash<T: Config>(msg_hash: &H256) -> Result<ecdsa::Signature, DispatchError> {
    let msg_hash_string = hex::encode(msg_hash);
    let confirmation = AVN::<T>::request_ecdsa_signature_from_external_service(&msg_hash_string)?;
    Ok(confirmation)
}

pub fn verify_signature<T: Config>(
    msg_hash: H256,
    author: &Author<T>,
    confirmation: &ecdsa::Signature,
) -> Result<(), Error<T>> {
    if !AVN::<T>::eth_signature_is_valid(hex::encode(msg_hash), author, confirmation) {
        Err(Error::<T>::InvalidECDSASignature)
    } else {
        Ok(())
    }
}

pub fn send_tx<T: Config>(tx: &ActiveTransactionData<T>) -> Result<H256, DispatchError> {
    match generate_send_calldata::<T>(tx) {
        Ok(calldata) => match send_transaction::<T>(calldata, &tx.data.sender) {
            Ok(eth_tx_hash) => Ok(eth_tx_hash),
            Err(_) => Err(Error::<T>::SendTransactionFailed.into()),
        },
        Err(_) => Err(Error::<T>::InvalidSendCalldata.into()),
    }
}

pub fn corroborate<T: Config>(
    tx: &ActiveTransactionData<T>,
    author: &Author<T>,
) -> Result<(Option<bool>, Option<bool>), DispatchError> {
    let status = check_tx_status::<T>(tx, author)?;
    if status.is_some() {
        let (tx_hash_is_valid, confirmations) = check_tx_hash::<T>(tx, author)?;
        if tx_hash_is_valid && confirmations.unwrap_or_default() < T::MinEthBlockConfirmation::get()
        {
            log::warn!("🚨 Transaction {:?} doesn't have the minimum eth confirmations yet, skipping corroboration. Current confirmation: {:?}",
                tx.request.tx_id, confirmations
            );
            return Ok((None, None))
        }

        return Ok((status, Some(tx_hash_is_valid)))
    }

    return Ok((None, None))
}

pub fn check_vow_reference_rate<T: Config>(author: &Author<T>, eth_block: Option<u32>) -> Result<U256, DispatchError> {
    if let Ok(calldata) = generate_check_reference_rate_calldata::<T>() {
        if let Ok(result) = call_check_reference_rate_method::<T>(calldata, &author.account_id, eth_block) {
            return Ok(result);
        } else {
            return Err(Error::<T>::CorroborateCallFailed.into())
        }
    }
    Err(Error::<T>::InvalidCorroborateCalldata.into())
}

fn check_tx_status<T: Config>(
    tx: &ActiveTransactionData<T>,
    author: &Author<T>,
) -> Result<Option<bool>, DispatchError> {
    if let Ok(calldata) = generate_corroborate_calldata::<T>(tx.request.tx_id, tx.data.expiry) {
        if let Ok(result) = call_corroborate_method::<T>(calldata, &author.account_id) {
            match result {
                0 => return Ok(None),
                1 => return Ok(Some(true)),
                -1 => return Ok(Some(false)),
                _ => return Err(Error::<T>::InvalidCorroborateResult.into()),
            }
        } else {
            return Err(Error::<T>::CorroborateCallFailed.into())
        }
    }
    Err(Error::<T>::InvalidCorroborateCalldata.into())
}

fn check_tx_hash<T: Config>(
    tx: &ActiveTransactionData<T>,
    author: &Author<T>,
) -> Result<(bool, Option<u64>), DispatchError> {
    if tx.data.eth_tx_hash != H256::zero() {
        if let Ok((call_data, num_confirmations)) =
            get_transaction_call_data::<T>(tx.data.eth_tx_hash, &author.account_id)
        {
            let expected_call_data = generate_send_calldata(&tx)?;
            return Ok((hex::encode(expected_call_data) == call_data, Some(num_confirmations)))
        } else {
            return Err(Error::<T>::ErrorGettingEthereumCallData.into())
        }
    }
    return Ok((TX_HASH_INVALID, None))
}

pub fn encode_confirmations(
    confirmations: &BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
) -> Vec<u8> {
    let mut concatenated_confirmations = Vec::new();
    for conf in confirmations {
        concatenated_confirmations.extend_from_slice(conf.as_ref());
    }
    concatenated_confirmations
}

pub fn generate_send_calldata<T: Config>(
    tx: &ActiveTransactionData<T>,
) -> Result<Vec<u8>, Error<T>> {
    let concatenated_confirmations = encode_confirmations(&tx.confirmation.confirmations);
    let mut full_params = unbound_params(&tx.data.eth_tx_params);
    full_params.push((BYTES.to_vec(), concatenated_confirmations));

    abi_encode_function(&tx.request.function_name.as_slice(), &full_params)
}

fn generate_corroborate_calldata<T: Config>(
    tx_id: EthereumId,
    expiry: u64,
) -> Result<Vec<u8>, Error<T>> {
    let params = vec![
        (UINT32.to_vec(), tx_id.to_string().into_bytes()),
        (UINT256.to_vec(), expiry.to_string().into_bytes()),
    ];

    abi_encode_function(b"corroborate", &params)
}

fn generate_check_reference_rate_calldata<T: Config>() -> Result<Vec<u8>, Error<T>> {
    let params = vec![];

    abi_encode_function(b"checkReferenceRate", &params)
}

pub fn generate_encoded_lower_proof<T: Config>(
    lower_req: &LowerProofRequestData,
    confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
) -> Vec<u8> {
    let concatenated_confirmations = encode_confirmations(&confirmations);
    let mut compact_lower_data = Vec::new();
    compact_lower_data.extend_from_slice(&lower_req.params.to_vec());
    compact_lower_data.extend_from_slice(&concatenated_confirmations);

    return compact_lower_data
}

fn abi_encode_function<T: pallet::Config>(
    function_name: &[u8],
    params: &[(Vec<u8>, Vec<u8>)],
) -> Result<Vec<u8>, Error<T>> {
    let inputs = params
        .iter()
        .filter_map(|(type_bytes, _)| {
            to_param_type(type_bytes).map(|kind| Param { name: "".to_string(), kind })
        })
        .collect::<Vec<_>>();

    let tokens: Result<Vec<_>, _> = params
        .iter()
        .map(|(type_bytes, value_bytes)| {
            let param_type =
                to_param_type(type_bytes).ok_or_else(|| Error::<T>::ParamTypeEncodingError)?;
            to_token_type(&param_type, value_bytes)
        })
        .collect();

    let function = Function {
        name: core::str::from_utf8(function_name).unwrap().to_string(),
        inputs,
        outputs: Vec::<Param>::new(),
        constant: false,
    };

    function.encode_input(&tokens?).map_err(|_| Error::<T>::FunctionEncodingError)
}

pub fn to_param_type(key: &Vec<u8>) -> Option<ParamType> {
    match key.as_slice() {
        BYTES => Some(ParamType::Bytes),
        BYTES32 => Some(ParamType::FixedBytes(32)),
        UINT32 => Some(ParamType::Uint(32)),
        UINT128 => Some(ParamType::Uint(128)),
        UINT256 => Some(ParamType::Uint(256)),
        ADDRESS => Some(ParamType::Address),

        _ => None,
    }
}

/// Please note: `value` will accept any bytes and its up to the caller to ensure the bytes are
/// valid for `kind`. The compiler will not catch these errors at compile time, but can error at
/// runtime.
pub fn to_token_type<T: pallet::Config>(kind: &ParamType, value: &[u8]) -> Result<Token, Error<T>> {
    match kind {
        ParamType::Bytes => Ok(Token::Bytes(value.to_vec())),
        ParamType::Uint(_) => {
            let dec_str = core::str::from_utf8(value).map_err(|_| Error::<T>::InvalidUTF8)?;
            let dec_value = Int::from_dec_str(dec_str).map_err(|_| Error::<T>::InvalidUint)?;
            Ok(Token::Uint(dec_value))
        },
        ParamType::FixedBytes(size) => {
            if value.len() != *size {
                return Err(Error::<T>::InvalidBytes)
            }
            Ok(Token::FixedBytes(value.to_vec()))
        },
        ParamType::Address => Ok(Token::Address(Address::from_slice(value))),
        _ => Err(Error::<T>::InvalidParamData),
    }
}

fn get_transaction_call_data<T: Config>(
    eth_tx_hash: H256,
    author_account_id: &T::AccountId,
) -> Result<(String, u64), DispatchError> {
    let query_request =
        EthQueryRequest { tx_hash: eth_tx_hash, response_type: EthQueryResponseType::CallData };
    make_ethereum_call::<(String, u64), T>(
        author_account_id,
        "query",
        query_request.encode(),
        process_query_result::<T>,
        None
    )
}

fn send_transaction<T: Config>(
    calldata: Vec<u8>,
    author_account_id: &T::AccountId,
) -> Result<H256, DispatchError> {
    make_ethereum_call::<H256, T>(author_account_id, "send", calldata, process_tx_hash::<T>, None)
}

fn call_corroborate_method<T: Config>(
    calldata: Vec<u8>,
    author_account_id: &T::AccountId,
) -> Result<i8, DispatchError> {
    make_ethereum_call::<i8, T>(
        author_account_id,
        "view",
        calldata,
        process_corroborate_result::<T>,
        None
    )
}

fn call_check_reference_rate_method<T: Config>(
    calldata: Vec<u8>,
    author_account_id: &T::AccountId,
    eth_block: Option<u32>,
) -> Result<U256, DispatchError> {
    make_ethereum_call::<U256, T>(
        author_account_id,
        "view",
        calldata,
        process_check_reference_rate_result::<T>,
        eth_block,
    )
}

fn make_ethereum_call<R, T: Config>(
    author_account_id: &T::AccountId,
    endpoint: &str,
    calldata: Vec<u8>,
    process_result: fn(Vec<u8>) -> Result<R, DispatchError>,
    eth_block: Option<u32>,
) -> Result<R, DispatchError> {
    let sender = T::AccountToBytesConvert::into_bytes(&author_account_id);
    let contract_address = AVN::<T>::get_bridge_contract_address();
    let ethereum_call = EthTransaction::new(sender, contract_address, calldata);
    let url_path = eth_block
        .map(|block| format!("eth/{}/{}", endpoint, block))
        .unwrap_or_else(|| format!("eth/{}", endpoint));

    let result = AVN::<T>::post_data_to_service(url_path, ethereum_call.encode())?;
    process_result(result)
}

fn process_tx_hash<T: Config>(result: Vec<u8>) -> Result<H256, DispatchError> {
    if result.len() != 64 {
        return Err(Error::<T>::InvalidHashLength.into())
    }

    let tx_hash_string = core::str::from_utf8(&result).map_err(|_| Error::<T>::InvalidUTF8)?;

    let mut data: [u8; 32] = [0; 32];
    hex::decode_to_slice(tx_hash_string, &mut data[..])
        .map_err(|_| Error::<T>::InvalidHexString)?;

    Ok(H256::from_slice(&data))
}

fn process_corroborate_result<T: Config>(result: Vec<u8>) -> Result<i8, DispatchError> {
    let result_bytes = hex::decode(&result).map_err(|_| Error::<T>::InvalidBytes)?;

    if result_bytes.len() != 32 {
        return Err(Error::<T>::InvalidBytesLength.into())
    }

    Ok(result_bytes[31] as i8)
}

fn process_check_reference_rate_result<T: Config>(result: Vec<u8>) -> Result<U256, DispatchError> {
    let result_bytes = hex::decode(&result).map_err(|_| Error::<T>::InvalidBytes)?;

    if result_bytes.len() != 32 {
        return Err(Error::<T>::InvalidBytesLength.into())
    }

    let u256_value = U256::from_big_endian(&result_bytes);

    Ok(u256_value)
}

fn process_query_result<T: Config>(result: Vec<u8>) -> Result<(String, u64), DispatchError> {
    let result_bytes = hex::decode(&result).map_err(|_| Error::<T>::InvalidBytes)?;
    let (call_data, eth_tx_confirmations) = try_process_query_result::<Vec<u8>, T>(result_bytes)
        .map_err(|e| {
            log::error!("❌ Error processing query result from Ethereum: {:?}", e);
            e
        })?;

    Ok((hex::encode(call_data), eth_tx_confirmations))
}
