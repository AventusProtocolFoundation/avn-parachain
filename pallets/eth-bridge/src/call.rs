use super::*;
use crate::{Author, Config};
use sp_core::{ecdsa, H256};

pub fn add_confirmation<T: Config>(
    request_id: EthereumId,
    confirmation: ecdsa::Signature,
    author: Author<T>,
) {
    let proof = add_confirmation_proof::<T>(request_id, &confirmation, &author.account_id);
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call = Call::<T>::add_confirmation { request_id, confirmation, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

pub fn add_eth_tx_hash<T: Config>(tx_id: EthereumId, eth_tx_hash: H256, author: Author<T>) {
    let proof = add_eth_tx_hash_proof::<T>(tx_id, &eth_tx_hash, &author.account_id);
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call = Call::<T>::add_eth_tx_hash { tx_id, eth_tx_hash, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

pub fn add_corroboration<T: Config>(
    tx_id: EthereumId,
    tx_succeeded: bool,
    tx_hash_is_valid: bool,
    author: Author<T>,
) {
    let proof =
        add_corroboration_proof::<T>(tx_id, tx_succeeded, tx_hash_is_valid, &author.account_id);
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call =
        Call::<T>::add_corroboration { tx_id, tx_succeeded, tx_hash_is_valid, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

pub fn submit_ethereum_events<T: Config>(
    author: Author<T>,
    events_partition: EthereumEventsPartition,
    signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
) -> Result<(), ()> {
    let call = Call::<T>::submit_ethereum_events { author, events_partition, signature };
    SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
}

pub fn submit_latest_ethereum_block<T: Config>(
    author: Author<T>,
    latest_seen_block: u32,
    signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
) -> Result<(), ()> {
    let call = Call::<T>::submit_latest_ethereum_block { author, latest_seen_block, signature };
    SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
}

pub fn submit_read_result<T: Config>(
    read_id: u32,
    bytes: BoundedVec<u8, ReadBytesLimit>,
    author: Author<T>,
) {
    let proof = submit_read_result_proof::<T>(read_id, &bytes, &author.account_id);
    let signature = author.key.sign(&proof).expect("Error signing proof");

    let call = Call::<T>::submit_read_result { read_id, bytes, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

fn add_confirmation_proof<T: Config>(
    tx_id: EthereumId,
    confirmation: &ecdsa::Signature,
    account_id: &T::AccountId,
) -> Vec<u8> {
    (ADD_CONFIRMATION_CONTEXT, tx_id, confirmation, &account_id).encode()
}

fn add_eth_tx_hash_proof<T: Config>(
    tx_id: EthereumId,
    eth_tx_hash: &H256,
    account_id: &T::AccountId,
) -> Vec<u8> {
    (ADD_ETH_TX_HASH_CONTEXT, tx_id, *eth_tx_hash, &account_id).encode()
}

fn add_corroboration_proof<T: Config>(
    tx_id: EthereumId,
    tx_succeeded: bool,
    tx_hash_is_valid: bool,
    account_id: &T::AccountId,
) -> Vec<u8> {
    (ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, tx_hash_is_valid, &account_id).encode()
}

fn submit_read_result_proof<T: Config>(
    read_id: u32,
    bytes: &BoundedVec<u8, ReadBytesLimit>,
    account_id: &T::AccountId,
) -> Vec<u8> {
    (SUBMIT_READ_RESULT_CONTEXT, read_id, bytes, &account_id).encode()
}
