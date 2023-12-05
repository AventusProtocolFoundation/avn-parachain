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

    let id = tx::use_next_tx_id::<T>();

    let send_req = SendRequestData {
        id,
        function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
            .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
        params: bound_params(&params.to_vec())?,
    };

    if ActiveRequest::<T>::get().is_some() {
        queue_request(Request::Send(send_req))?;
    } else {
        tx::set_up_active_tx(send_req)?;
    }

    Ok(id)
}

pub fn replay_transaction<T: Config>(mut tx: ActiveTransactionData<T>) -> Result<(), Error<T>> {
    tx.request_data.id = tx::use_next_tx_id::<T>();
    Ok(tx::set_up_active_tx(tx.request_data)?)
}

pub fn process_next_request<T: Config>() -> Result<(), Error<T>> {
    ActiveRequest::<T>::kill();

    if let Some(req) = request::dequeue_request::<T>() {
        return match req {
            Request::Send(send_req) => tx::set_up_active_tx(send_req),
            Request::LowerProof(lower_proof_req) => /*Implement me*/ Ok(()),
        }
    };

    Ok(())
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

pub fn dequeue_request<T: Config>() -> Option<Request> {
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
