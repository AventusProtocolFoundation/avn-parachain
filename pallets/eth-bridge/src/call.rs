use super::*;
use crate::{Author, Config};
use sp_core::{ecdsa, H256};

pub fn add_confirmation<T: Config>(tx_id: u32, confirmation: ecdsa::Signature, author: Author<T>) {
    let proof = add_confirmation_proof::<T>(tx_id, &confirmation, &author);
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call = Call::<T>::add_confirmation { tx_id, confirmation, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

pub fn add_eth_tx_hash<T: Config>(tx_id: u32, eth_tx_hash: H256, author: Author<T>) {
    let proof = add_eth_tx_hash_proof::<T>(tx_id, &eth_tx_hash, &author);
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call = Call::<T>::add_eth_tx_hash { tx_id, eth_tx_hash, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

pub fn add_corroboration<T: Config>(tx_id: u32, tx_succeeded: bool, author: Author<T>) {
    let proof = add_corroboration_proof::<T>(tx_id, tx_succeeded, &author);
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call = Call::<T>::add_corroboration { tx_id, tx_succeeded, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

fn add_confirmation_proof<T: Config>(
    tx_id: u32,
    confirmation: &ecdsa::Signature,
    author: &Author<T>,
) -> Vec<u8> {
    (ADD_CONFIRMATION_CONTEXT, tx_id, confirmation, &author.account_id).encode()
}

fn add_eth_tx_hash_proof<T: Config>(tx_id: u32, eth_tx_hash: &H256, author: &Author<T>) -> Vec<u8> {
    (ADD_ETH_TX_HASH_CONTEXT, tx_id, *eth_tx_hash, &author.account_id).encode()
}

fn add_corroboration_proof<T: Config>(
    tx_id: u32,
    tx_succeeded: bool,
    author: &Author<T>,
) -> Vec<u8> {
    (ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, &author.account_id).encode()
}