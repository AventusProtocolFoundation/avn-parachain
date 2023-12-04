use super::*;
use crate::{util::bound_params, Config};
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

    let send_req = SendRequestData {
        id,
        function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
            .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
        params: bound_params(&params.to_vec())?,
    };

    if ActiveRequest::<T>::get().is_some() {
        request::queue_tx_request(send_req)?;
    } else {
        tx::set_up_active_tx(send_req)?;
    }

    Ok(id)
}

pub fn replay_transaction<T: Config>(mut tx: ActiveTransactionData<T>) -> Result<(), Error<T>> {
    tx.request_data.id = use_next_tx_id::<T>();
    Ok(tx::set_up_active_tx(tx.request_data)?)
}

pub fn queue_tx_request<T: Config>(tx_request: SendRequestData) -> Result<(), Error<T>> {
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

pub fn dequeue_tx_request<T: Config>() -> Option<SendRequestData> {
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

fn use_next_tx_id<T: Config>() -> u32 {
    let tx_id = NextTxId::<T>::get();
    NextTxId::<T>::put(tx_id + 1);
    tx_id
}
