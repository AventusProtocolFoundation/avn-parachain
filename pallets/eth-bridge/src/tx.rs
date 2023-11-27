use super::*;
use crate::{offence::create_and_report_corroboration_offence, util::bound_params, Config};
use frame_support::BoundedVec;
use sp_core::Get;

pub fn add_new_send_request<T: Config>(
    function_name: &[u8],
    params: &[(Vec<u8>, Vec<u8>)],
) -> Result<EthereumId, Error<T>> {
    let function_name_string =
        String::from_utf8(function_name.to_vec()).map_err(|_| Error::<T>::FunctionNameError)?;
    if function_name_string.is_empty() {
        return Err(Error::<T>::EmptyFunctionName)
    }

    let id = use_next_tx_id::<T>();

    let tx_request: Request = Request::Send(SendRequest {
        id,
        function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
            .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
        params: bound_params(&params.to_vec())?,
    });

    if ActiveConfirmation::<T>::get().is_some() {
        queue_request(tx_request)?;
    } else {
        set_up_active_tx(tx_request)?;
    }

    Ok(id)
}

pub fn is_active_confirmation<T: Config>(id: EthereumId) -> bool {
    ActiveConfirmation::<T>::get().map_or(false, |data_to_confirm| {
        util::get_request_id::<T>(&data_to_confirm.request) == Some(id)
    })
}

pub fn is_active_transaction<T: Config>(tx_id: EthereumId) -> bool {
    ActiveTransaction::<T>::get().map_or(false, |data_to_confirm| {
        util::get_request_id::<T>(&data_to_confirm.request) == Some(tx_id)
    })
}

fn replay_transaction<T: Config>(mut tx: ActiveTransactionData<T>) -> Result<(), Error<T>> {
    if let Request::Send(mut r) = tx.request {
        r.id = use_next_tx_id::<T>();
        tx.request = Request::Send(r);
        return Ok(set_up_active_tx(tx.request)?)
    }

    Err(Error::<T>::InvalidRequest)
}

fn complete_transaction<T: Config>(
    mut tx: ActiveTransactionData<T>,
    success: bool,
) -> Result<(), Error<T>> {
    // Alert the originating pallet:
    T::OnBridgePublisherResult::process_result(tx.id(), success)
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
    SettledTransactions::<T>::insert(tx.id(), tx.data);

    if let Some(tx_request) = dequeue_tx_request::<T>() {
        set_up_active_tx(tx_request)?;
    } else {
        ActiveTransaction::<T>::kill();
    }

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
        return Ok(replay_transaction(tx)?)
    }

    Ok(complete_transaction::<T>(tx, success)?)
}

fn queue_request<T: Config>(request: Request) -> Result<(), Error<T>> {
    RequestQueue::<T>::mutate(|maybe_queue| {
        let mut queue: Vec<_> = maybe_queue.clone().unwrap_or_else(Default::default).into();

        if queue.len() < T::MaxQueuedTxRequests::get() as usize {
            queue.push(request);
            let bounded_queue =
                BoundedVec::try_from(queue).expect("Size known to be in bounds here");
            *maybe_queue = Some(bounded_queue);
            Ok(())
        } else {
            Err(Error::<T>::TxRequestQueueFull)
        }
    })
}

fn dequeue_tx_request<T: Config>() -> Option<Request> {
    let mut queue = <RequestQueue<T>>::take();

    let next_tx_request = match &mut queue {
        Some(q) if !q.is_empty() => Some(q.remove(0)),
        _ => None,
    };

    if let Some(q) = &queue {
        if !q.is_empty() {
            RequestQueue::<T>::put(q);
        }
    }

    next_tx_request
}

fn set_up_active_tx<T: Config>(tx_request: Request) -> Result<(), Error<T>> {
    let expiry = util::time_now::<T>() + EthTxLifetimeSecs::<T>::get();

    if let Request::Send(r) = &tx_request {
        let data = eth::create_send_tx_data(&r, expiry)?;
        let msg_hash = eth::generate_msg_hash(&data)?;

        ActiveTransaction::<T>::put(ActiveTransactionData {
            request: tx_request,
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

        return Ok(())
    }

    Err(Error::<T>::InvalidRequest)
}

fn use_next_tx_id<T: Config>() -> u32 {
    let tx_id = NextTxId::<T>::get();
    NextTxId::<T>::put(tx_id + 1);
    tx_id
}
