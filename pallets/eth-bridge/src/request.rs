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

    let tx_id = tx::use_next_tx_id::<T>();

    let send_req = SendRequestData {
        tx_id,
        function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
            .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
        params: bound_params(&params.to_vec())?,
    };

    if ActiveRequest::<T>::get().is_some() {
        queue_request(Request::Send(send_req))?;
    } else {
        tx::set_up_active_tx(send_req)?;
    }

    Ok(tx_id)
}

pub fn add_new_lower_proof_request<T: Config>(
    lower_id: LowerId,
    params: &[(Vec<u8>, Vec<u8>)]
) -> Result<(), Error<T>> {

    let proof_req = LowerProofRequestData {
        lower_id,
        params: bound_params(&params.to_vec())?,
    };

    if ActiveRequest::<T>::get().is_some() {
        queue_request(Request::LowerProof(proof_req))?;
    } else {
        set_up_active_lower_proof(proof_req)?;
    }

    Ok(())
}

pub fn process_next_request<T: Config>() -> Result<(), Error<T>> {
    ActiveRequest::<T>::kill();

    if let Some(req) = request::dequeue_request::<T>() {
        return match req {
            Request::Send(send_req) => tx::set_up_active_tx(send_req),
            Request::LowerProof(lower_proof_req) => set_up_active_lower_proof(lower_proof_req),
        }
    };

    Ok(())
}

pub fn has_enough_confirmations<T: Config>(req: &ActiveRequestData<T>) -> bool {
    let confirmations = req.confirmation.confirmations.len() as u32;
    match req.request {
        // The sender's confirmation is implicit so we only collect them from other authors:
        Request::Send(_) => util::has_enough_confirmations::<T>(confirmations),
        Request::LowerProof(_) => util::has_supermajority_confirmations::<T>(confirmations),
    }
}

pub fn complete_lower_proof_request<T: Config>(lower_req: &LowerProofRequestData, confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>) -> Result<(), Error<T>> {
    // Write the data to permanent storage:
    let lower_proof = eth::generate_abi_encoded_lower_proof(lower_req, confirmations)?;

    LowersReadyToClaim::<T>::insert(
        lower_req.lower_id,
        LowerProofData {
            params: lower_req.params.clone(),
            lower_data: BoundedVec::<u8, LowerDataLimit>::try_from(lower_proof).map_err(|_| Error::<T>::LowerDataLimitExceeded)?,
        }
    );

    <crate::Pallet<T>>::deposit_event(Event::<T>::LowerReadyToClaim {
        lower_id: lower_req.lower_id
    });

    // Process any new request from the queue
    request::process_next_request::<T>()?;

    Ok(())
}

fn set_up_active_lower_proof<T: Config>(req: LowerProofRequestData) -> Result<(), Error<T>> {
    let msg_hash = eth::generate_msg_hash(&req.params)?;

    ActiveRequest::<T>::put(ActiveRequestData {
        request: Request::LowerProof(req),
        confirmation: ActiveConfirmation { msg_hash, confirmations: BoundedVec::default() },
        tx_data: None,
        last_updated: <frame_system::Pallet<T>>::block_number(),
    });

    return Ok(())
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

fn dequeue_request<T: Config>() -> Option<Request> {
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
