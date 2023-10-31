use super::*;
use crate::{
    util::{bound_params, unbound_params},
    Author, Config, AVN,
};
use ethabi::{Function, Int, Param, ParamType, Token};
use pallet_avn::AccountToBytesConverter;
use sp_avn_common::EthTransaction;
use sp_core::{ecdsa, H256};
use sp_runtime::DispatchError;
use sp_std::vec;

const UINT256: &[u8] = b"uint256";
const UINT128: &[u8] = b"uint128";
const UINT32: &[u8] = b"uint32";
const BYTES: &[u8] = b"bytes";
const BYTES32: &[u8] = b"bytes32";

pub fn create_tx_data<T: Config>(
    tx_request: &RequestData,
    expiry: u64,
) -> Result<TransactionData<T>, Error<T>> {
    let mut extended_params = unbound_params(&tx_request.params);
    extended_params.push((UINT256.to_vec(), expiry.to_string().into_bytes()));
    extended_params.push((UINT32.to_vec(), tx_request.tx_id.to_string().into_bytes()));

    let tx_data = TransactionData {
        function_name: tx_request.function_name.clone(),
        params: bound_params(&extended_params)?,
        sender: assign_sender()?,
        eth_tx_hash: H256::zero(),
        tx_succeeded: false,
    };

    Ok(tx_data)
}

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

pub fn send_tx<T: Config>(
    tx: &ActiveTransactionData<T>,
    author: &Author<T>,
) -> Result<H256, DispatchError> {
    match generate_send_calldata::<T>(tx) {
        Ok(calldata) => match send_transaction::<T>(calldata, author) {
            Ok(eth_tx_hash) => Ok(eth_tx_hash),
            Err(_) => Err(Error::<T>::SendTransactionFailed.into()),
        },
        Err(_) => Err(Error::<T>::InvalidSendCalldata.into()),
    }
}

pub fn check_tx_status<T: Config>(
    tx_id: u32,
    expiry: u64,
    author: &Author<T>,
) -> Result<Option<bool>, DispatchError> {
    if let Ok(calldata) = generate_corroborate_calldata::<T>(tx_id, expiry) {
        if let Ok(result) = call_corroborate_method::<T>(calldata, author) {
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

pub fn generate_msg_hash<T: pallet::Config>(
    tx_data: &TransactionData<T>,
) -> Result<H256, Error<T>> {
    let params = unbound_params(&tx_data.params);
    let tokens: Result<Vec<_>, _> = params
        .iter()
        .map(|(type_bytes, value_bytes)| {
            let param_type = to_param_type(type_bytes).ok_or_else(|| Error::<T>::MsgHashError)?;
            to_token_type(&param_type, value_bytes)
        })
        .collect();

    let encoded = ethabi::encode(&tokens?);
    let msg_hash = keccak_256(&encoded);

    Ok(H256::from(msg_hash))
}

pub fn generate_send_calldata<T: Config>(
    tx: &ActiveTransactionData<T>,
) -> Result<Vec<u8>, Error<T>> {
    let mut concatenated_confirmations = Vec::new();
    for conf in &tx.confirmations {
        concatenated_confirmations.extend_from_slice(conf.as_ref());
    }

    let mut full_params = unbound_params(&tx.data.params);
    full_params.push((BYTES.to_vec(), concatenated_confirmations));

    abi_encode_function(&tx.data.function_name.as_slice(), &full_params)
}

fn generate_corroborate_calldata<T: Config>(tx_id: u32, expiry: u64) -> Result<Vec<u8>, Error<T>> {
    let params = vec![
        (UINT32.to_vec(), tx_id.to_string().into_bytes()),
        (UINT256.to_vec(), expiry.to_string().into_bytes()),
    ];

    abi_encode_function(b"corroborate", &params)
}

fn assign_sender<T: Config>() -> Result<T::AccountId, Error<T>> {
    let current_block_number = <frame_system::Pallet<T>>::block_number();

    match AVN::<T>::calculate_primary_validator(current_block_number) {
        Ok(primary_validator) => {
            let sender = primary_validator;
            Ok(sender)
        },
        Err(_) => Err(Error::<T>::ErrorAssigningSender),
    }
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

fn to_param_type(key: &Vec<u8>) -> Option<ParamType> {
    match key.as_slice() {
        BYTES => Some(ParamType::Bytes),
        BYTES32 => Some(ParamType::FixedBytes(32)),
        UINT32 => Some(ParamType::Uint(32)),
        UINT128 => Some(ParamType::Uint(128)),
        UINT256 => Some(ParamType::Uint(256)),

        _ => None,
    }
}

/// Please note: `value` will accept any bytes and its up to the caller to ensure the bytes are valid for `kind`.
/// The compiler will not catch these errors at compile time, but can error at runtime.
fn to_token_type<T: pallet::Config>(kind: &ParamType, value: &[u8]) -> Result<Token, Error<T>> {
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
        _ => Err(Error::<T>::InvalidParamData),
    }
}

fn send_transaction<T: Config>(
    calldata: Vec<u8>,
    author: &Author<T>,
) -> Result<H256, DispatchError> {
    make_ethereum_call::<H256, T>(author, "send", calldata, process_tx_hash::<T>)
}

fn call_corroborate_method<T: Config>(
    calldata: Vec<u8>,
    author: &Author<T>,
) -> Result<i8, DispatchError> {
    make_ethereum_call::<i8, T>(author, "view", calldata, process_corroborate_result::<T>)
}

fn make_ethereum_call<R, T: Config>(
    author: &Author<T>,
    endpoint: &str,
    calldata: Vec<u8>,
    process_result: fn(Vec<u8>) -> Result<R, DispatchError>,
) -> Result<R, DispatchError> {
    let sender = T::AccountToBytesConvert::into_bytes(&author.account_id);
    let contract_address = AVN::<T>::get_bridge_contract_address();
    let ethereum_call = EthTransaction::new(sender, contract_address, calldata);
    let url_path = format!("eth/{}", endpoint);
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

    log::info!("PROCESS SEND RESPONSE !!! {:?}", result_bytes);

    Ok(result_bytes[31] as i8)
}
