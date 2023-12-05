use super::*;
use crate::{offence::create_and_report_corroboration_offence, util::bound_params, Config};
use frame_support::BoundedVec;
use sp_core::Get;

pub fn is_active<T: Config>(tx_id: EthereumId) -> bool {
    ActiveRequest::<T>::get().map_or(false, |active_tx| active_tx.id == tx_id)
}

fn complete_transaction<T: Config>(
    mut tx: ActiveTransactionData<T>,
    success: bool,
) -> Result<(), Error<T>> {
    // Alert the originating pallet:
    T::OnBridgePublisherResult::process_result(tx.id, success)
        .map_err(|_| Error::<T>::HandlePublishingResultFailed)?;

    tx.data.tx_succeeded = success;

    // Check for offences:
    if success {
        if !tx.failure_corroborations.is_empty() {
            create_and_report_corroboration_offence::<T>(
                &tx.data.sender,
                &tx.failure_corroborations,
                offence::CorroborationOffenceType::ChallengeAttemptedOnSuccessfulTransaction,
            )
        }

        // if the transaction is a success but the eth tx hash is wrong remove it
        if util::has_enough_corroborations::<T>(tx.invalid_tx_hash_corroborations.len()) {
            tx.data.eth_tx_hash = H256::zero();
        }
    } else {
        if !tx.success_corroborations.is_empty() {
            create_and_report_corroboration_offence::<T>(
                &tx.data.sender,
                &tx.success_corroborations,
                offence::CorroborationOffenceType::ChallengeAttemptedOnUnsuccessfulTransaction,
            )
        }
    }

    // Write the tx data to permanent storage:
    SettledTransactions::<T>::insert(tx.id, tx.data);

    // Process any new request from the queue
    request::process_next_request::<T>()?;

    Ok(())
}

pub fn finalize_state<T: Config>(
    tx: ActiveTransactionData<T>,
    success: bool,
) -> Result<(), Error<T>> {
    // if the transaction failed and the tx hash is missing or pointing to a different transaction,
    // replay transaction
    if !success && util::has_enough_corroborations::<T>(tx.invalid_tx_hash_corroborations.len()) {
        // raise an offence on the "sender" because the tx_hash they provided was invalid
        return Ok(replay_send_request(tx)?)
    }

    Ok(complete_transaction::<T>(tx, success)?)
}

pub fn set_up_active_tx<T: Config>(tx_request: SendRequestData) -> Result<(), Error<T>> {
    let expiry = util::time_now::<T>() + EthTxLifetimeSecs::<T>::get();
    let data = eth::create_tx_data(&tx_request, expiry)?;
    let msg_hash = eth::generate_msg_hash(&data)?;

    ActiveRequest::<T>::put(ActiveTransactionData {
        id: tx_request.id,
        request_data: tx_request,
        data,
        expiry,
        msg_hash,
        last_updated: <frame_system::Pallet<T>>::block_number(),
        confirmations: BoundedVec::default(),
        success_corroborations: BoundedVec::default(),
        failure_corroborations: BoundedVec::default(),
        valid_tx_hash_corroborations: BoundedVec::default(),
        invalid_tx_hash_corroborations: BoundedVec::default(),
    });

    Ok(())
}
pub fn replay_send_request<T: Config>(mut tx: ActiveTransactionData<T>) -> Result<(), Error<T>> {
    tx.id = use_next_tx_id::<T>();
    return Ok(set_up_active_tx(tx.request_data)?)
}

pub fn use_next_tx_id<T: Config>() -> u32 {
    let tx_id = NextTxId::<T>::get();
    NextTxId::<T>::put(tx_id + 1);
    tx_id
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