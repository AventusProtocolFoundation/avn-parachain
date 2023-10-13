use super::*;
use crate::{Author, Config};
use sp_core::{ecdsa, H256};

pub fn add_confirmation<T: Config>(tx_id: u32, confirmation: ecdsa::Signature, author: Author<T>) {
    let proof = add_confirmation_proof::<T>(tx_id, confirmation.clone(), author.clone());
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call = Call::<T>::add_confirmation { tx_id, confirmation, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

pub fn add_receipt<T: Config>(tx_id: u32, eth_tx_hash: H256, author: Author<T>) {
    let proof = add_receipt_proof::<T>(tx_id, eth_tx_hash, author.clone());
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call = Call::<T>::add_receipt { tx_id, eth_tx_hash, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

pub fn add_corroboration<T: Config>(tx_id: u32, tx_succeeded: bool, author: Author<T>) {
    let proof = add_corroboration_proof::<T>(tx_id, tx_succeeded, author.clone());
    let signature = author.key.sign(&proof).expect("Error signing proof");
    let call = Call::<T>::add_corroboration { tx_id, tx_succeeded, author, signature };
    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
}

fn add_confirmation_proof<T: Config>(
    tx_id: u32,
    confirmation: ecdsa::Signature,
    author: Author<T>,
) -> Vec<u8> {
    (ADD_CONFIRMATION_CONTEXT, tx_id, confirmation, author.account_id).encode()
}

fn add_receipt_proof<T: Config>(tx_id: u32, eth_tx_hash: H256, author: Author<T>) -> Vec<u8> {
    (ADD_RECEIPT_CONTEXT, tx_id, eth_tx_hash, author.account_id).encode()
}

fn add_corroboration_proof<T: Config>(
    tx_id: u32,
    tx_succeeded: bool,
    author: Author<T>,
) -> Vec<u8> {
    (ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, author.account_id).encode()
}
