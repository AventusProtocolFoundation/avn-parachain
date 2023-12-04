use super::*;
use crate::{offence::create_and_report_corroboration_offence, util::bound_params, Config};
use frame_support::BoundedVec;
use sp_core::Get;

pub fn add_new_request<T: Config>(
    function_name: &[u8],
    params: &[(Vec<u8>, Vec<u8>)],
) -> Result<EthereumId, Error<T>> {
    let function_name_string =
        String::from_utf8(function_name.to_vec()).map_err(|_| Error::<T>::FunctionNameError)?;
    if function_name_string.is_empty() {
        return Err(Error::<T>::EmptyFunctionName)
    }

    let tx_id = use_next_tx_id::<T>();

    let tx_request = Request {
        tx_id,
        function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
            .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
        params: bound_params(&params.to_vec())?,
    };

    if ActiveRequest::<T>::get().is_some() {
        queue_tx_request(tx_request)?;
    } else {
        set_up_active_tx(tx_request)?;
    }

    Ok(tx_id)
}

pub fn is_active<T: Config>(tx_id: EthereumId) -> bool {
    ActiveRequest::<T>::get().map_or(false, |active_tx| active_tx.id == tx_id)
}

fn replay_transaction<T: Config>(mut tx: ActiveTransactionData<T>) -> Result<(), Error<T>> {
    tx.request_data.tx_id = use_next_tx_id::<T>();
    Ok(set_up_active_tx(tx.request_data)?)
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

    if let Some(tx_request) = dequeue_tx_request::<T>() {
        set_up_active_tx(tx_request)?;
    } else {
        ActiveRequest::<T>::kill();
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

fn queue_tx_request<T: Config>(tx_request: Request) -> Result<(), Error<T>> {
    RequestQueue::<T>::mutate(|maybe_queue| {
        let mut queue: Vec<_> = maybe_queue.clone().unwrap_or_else(Default::default).into();

        if queue.len() < T::MaxQueuedTxRequests::get() as usize {
            queue.push(tx_request);
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
    let data = eth::create_tx_data(&tx_request, expiry)?;
    let msg_hash = eth::generate_msg_hash(&data)?;

    ActiveRequest::<T>::put(ActiveTransactionData {
        id: tx_request.tx_id,
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

fn use_next_tx_id<T: Config>() -> EthereumId {
    let tx_id = NextTxId::<T>::get();
    NextTxId::<T>::put(tx_id + 1);
    tx_id
}
