use super::*;
use crate::{util::bound_params, Config};
use frame_support::BoundedVec;
use sp_core::Get;

pub fn add_new_request<T: Config>(
    function_name: &[u8],
    params: &[(Vec<u8>, Vec<u8>)],
) -> Result<u32, Error<T>> {
    let function_name_string =
        String::from_utf8(function_name.to_vec()).map_err(|_| Error::<T>::FunctionNameError)?;
    if function_name_string.is_empty() {
        return Err(Error::<T>::EmptyFunctionName)
    }

    let tx_id = use_next_tx_id::<T>();

    let tx_request = RequestData {
        tx_id,
        function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
            .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
        params: bound_params(&params.to_vec())?,
    };

    if ActiveTransaction::<T>::get().is_some() {
        queue_tx_request(tx_request)?;
    } else {
        set_as_active_tx(tx_request)?;
    }

    Ok(tx_id)
}

pub fn is_active<T: Config>(tx_id: &u32) -> bool {
    ActiveTransaction::<T>::get().map_or(false, |active_tx| active_tx.id == *tx_id)
}

pub fn finalize_state<T: Config>(
    mut active_tx: ActiveTransactionData<T>,
    success: bool,
) -> Result<(), Error<T>> {
    // Alert the originating pallet:
    T::OnPublishingResultHandler::process_result(active_tx.id, success)
        .map_err(|_| Error::<T>::HandlePublishingResultFailed)?;

    active_tx.data.tx_succeeded = success;
    // Write the tx details to permanent storage:
    SettledTransactions::<T>::insert(active_tx.id, active_tx.data);

    if let Some(tx_request) = dequeue_tx_request::<T>() {
        set_as_active_tx(tx_request)?;
    } else {
        ActiveTransaction::<T>::kill();
    }

    Ok(())
}

fn queue_tx_request<T: Config>(tx_request: RequestData) -> Result<(), Error<T>> {
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

fn dequeue_tx_request<T: Config>() -> Option<RequestData> {
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

fn set_as_active_tx<T: Config>(tx_request: RequestData) -> Result<(), Error<T>> {
    ActiveTransaction::<T>::put(ActiveTransactionData {
        id: tx_request.tx_id,
        data: eth::create_tx_data(&tx_request)?,
        success_corroborations: BoundedVec::default(),
        failure_corroborations: BoundedVec::default(),
    });

    Ok(())
}

fn use_next_tx_id<T: Config>() -> u32 {
    let tx_id = NextTxId::<T>::get();
    NextTxId::<T>::put(tx_id + 1);
    tx_id
}
