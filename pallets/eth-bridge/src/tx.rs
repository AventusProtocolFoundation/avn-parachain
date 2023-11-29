use super::*;
use crate::{offence::create_and_report_corroboration_offence, Config};
use frame_support::BoundedVec;

pub fn is_active_request<T: Config>(id: EthereumId) -> bool {
    ActiveTransaction::<T>::get().map_or(false, |d| d.request.id() == id)
}

fn complete_transaction<T: Config>(
    mut tx: ActiveTxRequestData<T>,
    success: bool,
) -> Result<(), Error<T>> {
    // Alert the originating pallet:
    T::OnBridgePublisherResult::process_result(tx.id(), success)
        .map_err(|_| Error::<T>::HandlePublishingResultFailed)?;

    tx.data.tx_succeeded = success;

    // Check for offences:
    if success {
        if !tx.data.failure_corroborations.is_empty() {
            create_and_report_corroboration_offence::<T>(
                &tx.data.sender,
                &tx.data.failure_corroborations,
                offence::CorroborationOffenceType::ChallengeAttemptedOnSuccessfulTransaction,
            )
        }

        // if the transaction is a success but the eth tx hash is wrong remove it
        if util::has_enough_corroborations::<T>(tx.data.invalid_tx_hash_corroborations.len()) {
            tx.data.eth_tx_hash = H256::zero();
        }
    } else {
        if !tx.data.success_corroborations.is_empty() {
            create_and_report_corroboration_offence::<T>(
                &tx.data.sender,
                &tx.data.success_corroborations,
                offence::CorroborationOffenceType::ChallengeAttemptedOnUnsuccessfulTransaction,
            )
        }
    }

    // Write the tx data to permanent storage:
    SettledTransactions::<T>::insert(
        tx.id(),
        TransactionData {
            function_name: tx.request.function_name,
            params: tx.request.params,
            sender: tx.data.sender,
            eth_tx_hash: tx.data.eth_tx_hash,
            tx_succeeded: tx.data.tx_succeeded,
        },
    );

    // Process any new request from the queue
    request::process_next_request::<T>()?;

    Ok(())
}

pub fn finalize_state<T: Config>(
    tx: ActiveTxRequestData<T>,
    success: bool,
) -> Result<(), Error<T>> {
    // if the transaction failed and the tx hash is missing or pointing to a different transaction,
    // replay transaction
    if !success &&
        util::has_enough_corroborations::<T>(tx.data.invalid_tx_hash_corroborations.len())
    {
        // raise an offence on the "sender" because the tx_hash they provided was invalid
        return Ok(request::replay_send_request(tx)?)
    }

    Ok(complete_transaction::<T>(tx, success)?)
}

pub fn set_up_active_tx<T: Config>(tx_request: Request) -> Result<(), Error<T>> {
    let expiry = util::time_now::<T>() + EthTxLifetimeSecs::<T>::get();

    if let Request::Send(ref req) = tx_request {
        let extended_params = req.extend_params(expiry)?;
        let msg_hash = eth::generate_msg_hash(&extended_params)?;

        ActiveTransaction::<T>::put(ActiveRequestData {
            request: tx_request.clone(),
            confirmation_data: ConfirmationData { msg_hash, confirmations: BoundedVec::default() },
            tx_data: Some(EthTransactionData {
                function_name: req.function_name.clone(),
                eth_tx_params: extended_params,
                expiry,
                eth_tx_hash: H256::zero(),
                sender: eth::assign_sender()?,
                success_corroborations: BoundedVec::default(),
                failure_corroborations: BoundedVec::default(),
                valid_tx_hash_corroborations: BoundedVec::default(),
                invalid_tx_hash_corroborations: BoundedVec::default(),
                tx_succeeded: false,
            }),
            last_updated: <frame_system::Pallet<T>>::block_number(),
        });

        return Ok(())
    }

    Err(Error::<T>::InvalidRequest)
}
