use super::*;
use crate::{Config, AVN};
use frame_support::{ensure, traits::UnixTime, BoundedVec};
use sp_avn_common::calculate_one_third_quorum;
use sp_core::H256;

pub fn use_next_tx_id<T: Config>() -> u32 {
    let tx_id = NextTxId::<T>::get();
    NextTxId::<T>::put(tx_id + 1);
    tx_id
}

pub fn time_now<T: Config>() -> u64 {
    <T as pallet::Config>::TimeProvider::now().as_secs()
}

pub fn quorum_reached<T: Config>(entries: u32) -> bool {
    let quorum = calculate_one_third_quorum(AVN::<T>::validators().len() as u32);
    entries >= quorum
}

pub fn add_new_tx_request<T: pallet::Config>(
    tx_id: u32,
    tx_data: TransactionData<T>,
) -> Result<(), Error<T>> {
    let corroborations =
        CorroborationData { tx_succeeded: BoundedVec::default(), tx_failed: BoundedVec::default() };

    Transactions::<T>::insert(tx_id, tx_data);
    Corroborations::<T>::insert(tx_id, corroborations);
    UnresolvedTxs::<T>::try_mutate(|txs| {
        txs.try_push(tx_id).map_err(|_| Error::<T>::UnresolvedTxLimitReached)
    })
}

pub fn assign_sender<T: Config>() -> Result<T::AccountId, Error<T>> {
    let current_block_number = <frame_system::Pallet<T>>::block_number();

    match AVN::<T>::calculate_primary_validator(current_block_number) {
        Ok(primary_validator) => {
            let sender = primary_validator;
            Ok(sender)
        },
        Err(_) => Err(Error::<T>::ErrorAssigningSender),
    }
}

pub fn update_confirmations<T: Config>(
    tx_id: u32,
    confirmation: &ecdsa::Signature,
    author: &Author<T>,
) -> Result<(), Error<T>> {
    let tx_data = Transactions::<T>::get(tx_id).ok_or(Error::<T>::TxIdNotFound)?;

    if !UnresolvedTxs::<T>::get().contains(&tx_id) ||
        author.account_id == tx_data.sender ||
        quorum_reached::<T>(tx_data.confirmations.len() as u32 + 1)
    {
        return Ok(())
    }

    eth::verify_signature::<T>(tx_data.msg_hash, &author, &confirmation)?;
    ensure!(!tx_data.confirmations.contains(confirmation), Error::<T>::DuplicateConfirmation);

    let mut updated_tx_data = tx_data;
    updated_tx_data
        .confirmations
        .try_push(confirmation.clone())
        .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;

    Transactions::<T>::insert(tx_id, updated_tx_data);

    Ok(())
}

pub fn set_eth_tx_hash<T: Config>(
    tx_id: u32,
    eth_tx_hash: H256,
    author: &Author<T>,
) -> Result<(), Error<T>> {
    let mut tx_data = Transactions::<T>::get(tx_id).ok_or(Error::<T>::TxIdNotFound)?;
    ensure!(tx_data.eth_tx_hash == H256::zero(), Error::<T>::EthTxHashAlreadySet);
    ensure!(tx_data.sender == author.account_id, Error::<T>::EthTxHashMustBeSetBySender);
    tx_data.eth_tx_hash = eth_tx_hash;
    Transactions::<T>::insert(tx_id, tx_data);
    Ok(())
}

pub fn requires_corroboration<T: Config>(tx_id: u32, author: &Author<T>) -> Result<bool, Error<T>> {
    let corroboration =
        Corroborations::<T>::get(tx_id).ok_or_else(|| Error::<T>::CorroborationNotFound)?;
    let not_in_succeeded = !corroboration.tx_succeeded.contains(&author.account_id);
    let not_in_failed = !corroboration.tx_failed.contains(&author.account_id);
    Ok(not_in_succeeded && not_in_falied)
}

pub fn update_corroborations<T: Config>(
    tx_id: u32,
    tx_succeeded: bool,
    author: &Author<T>,
) -> Result<(), Error<T>> {
    if !UnresolvedTxs::<T>::get().contains(&tx_id) {
        return Ok(())
    }

    let mut corroborations =
        Corroborations::<T>::get(tx_id).ok_or_else(|| Error::<T>::CorroborationNotFound)?;

    let (corroborations_that_agree, corroborations_that_disagree) = if tx_succeeded {
        (&mut corroborations.tx_succeeded, &corroborations.tx_failed)
    } else {
        (&mut corroborations.tx_failed, &corroborations.tx_succeeded)
    };

    if !corroborations_that_agree.contains(&author.account_id) &&
        !corroborations_that_disagree.contains(&author.account_id)
    {
        corroborations_that_agree
            .try_push(author.account_id.clone())
            .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;
    }

    if quorum_reached::<T>(corroborations_that_agree.len() as u32) {
        finalize::<T>(tx_id, tx_succeeded)?;
    } else {
        Corroborations::<T>::insert(tx_id, corroborations);
    }

    Ok(())
}

pub fn bound_params<T>(
    params: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<
    BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    Error<T>,
> {
    let result: Result<Vec<_>, _> = params
        .into_iter()
        .map(|(type_vec, value_vec)| {
            let type_bounded =
                BoundedVec::try_from(type_vec).map_err(|_| Error::<T>::TypeNameLengthExceeded)?;
            let value_bounded =
                BoundedVec::try_from(value_vec).map_err(|_| Error::<T>::ValueLengthExceeded)?;
            Ok::<_, Error<T>>((type_bounded, value_bounded))
        })
        .collect();

    BoundedVec::<_, ParamsLimit>::try_from(result?).map_err(|_| Error::<T>::ParamsLimitExceeded)
}

pub fn unbound_params(
    params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    params
        .into_iter()
        .map(|(type_bounded, value_bounded)| (type_bounded.into(), value_bounded.into()))
        .collect()
}

pub fn finalize<T: Config>(tx_id: u32, success: bool) -> Result<(), Error<T>> {
    // Alert the originating pallet and handle any error:
    T::OnPublishingResultHandler::process_result(tx_id, success)
        .map_err(|_| Error::<T>::HandlePublishingResultFailed)?;

    let mut tx_data = Transactions::<T>::get(tx_id).ok_or(Error::<T>::TxIdNotFound)?;
    tx_data.status = if success { EthStatus::Succeeded } else { EthStatus::Failed };

    UnresolvedTxs::<T>::mutate(|txs| {
        txs.retain(|&stored_tx_id| stored_tx_id != tx_id);
    });

    Corroborations::<T>::remove(tx_id);
    Transactions::<T>::insert(tx_id, tx_data);

    Ok(())
}
