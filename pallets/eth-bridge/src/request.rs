use super::*;
use crate::{util::bound_params, Config};
use frame_support::BoundedVec;
use sp_core::Get;
use sp_io::hashing::keccak_256;

pub fn add_new_send_request<T: Config>(
    function_name: &[u8],
    params: &[(Vec<u8>, Vec<u8>)],
    caller_id: &Vec<u8>,
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
        caller_id: BoundedVec::<_, CallerIdLimit>::try_from(caller_id.clone())
            .map_err(|_| Error::<T>::CallerIdLengthExceeded)?,
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
    params: &LowerParams,
    caller_id: &Vec<u8>,
) -> Result<(), Error<T>> {
    let proof_req = LowerProofRequestData {
        lower_id,
        params: *params,
        caller_id: BoundedVec::<_, CallerIdLimit>::try_from(caller_id.clone())
            .map_err(|_| Error::<T>::CallerIdLengthExceeded)?,
    };

    if ActiveRequest::<T>::get().is_some() {
        queue_request(Request::LowerProof(proof_req))?;
    } else {
        set_up_active_lower_proof(proof_req)?;
    }

    Ok(())
}

pub fn add_new_read_contract_request<T: Config>(
    contract_address: H160, // ✅ NEW
    function_name: &[u8],
    params: &[(Vec<u8>, Vec<u8>)],
    eth_block: Option<u32>,
    caller_id: &Vec<u8>,
) -> Result<u32, Error<T>> {
    let function_name_string =
        String::from_utf8(function_name.to_vec()).map_err(|_| Error::<T>::FunctionNameError)?;

    if function_name_string.is_empty() {
        return Err(Error::<T>::EmptyFunctionName)
    }

    // Reuse tx id generator for reads
    let read_id = tx::use_next_tx_id::<T>();

    let read_req = ReadContractRequestData {
        read_id,
        contract_address, // ✅ NEW
        function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
            .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
        params: bound_params(&params.to_vec())?,
        eth_block,
        caller_id: BoundedVec::<_, CallerIdLimit>::try_from(caller_id.clone())
            .map_err(|_| Error::<T>::CallerIdLengthExceeded)?,
    };

    if ActiveRequest::<T>::get().is_some() {
        queue_request(Request::ReadContract(read_req))?;
    } else {
        set_up_active_read_contract::<T>(read_req)?;
    }

    Ok(read_id)
}


// This function cannot error. Otherwise we need a way to resume processing queued requests.
pub fn process_next_request<T: Config>() {
    ActiveRequest::<T>::kill();

    if let Some(req) = request::dequeue_request::<T>() {
        match req {
            Request::Send(send_req) => {
                if let Err(e) = tx::set_up_active_tx::<T>(send_req.clone()) {
                    // If we failed to setup the next request, notify caller
                    log::error!(target: "runtime::eth-bridge", "Error processing send request from queue: {:?}", e);
                    let _ = T::BridgeInterfaceNotification::process_result(
                        send_req.tx_id,
                        send_req.caller_id.clone().into(),
                        false,
                    );
                    process_next_request::<T>();
                }
            },
            Request::LowerProof(lower_req) => {
                if let Err(e) = set_up_active_lower_proof::<T>(lower_req.clone()) {
                    // If we failed to setup the next request, notify caller
                    log::error!(target: "runtime::eth-bridge", "Error processing lower proof request from queue: {:?}", e);
                    let _ = T::BridgeInterfaceNotification::process_lower_proof_result(
                        lower_req.lower_id,
                        lower_req.caller_id.clone().into(),
                        Err(()),
                    );
                    process_next_request::<T>();
                }
            },
            Request::ReadContract(read_req) => {
                if let Err(e) = set_up_active_read_contract::<T>(read_req.clone()) {
                    log::error!(target: "runtime::eth-bridge", "Error processing read contract request from queue: {:?}", e);
            
                    // Requires callback added later (Step 3/4)
                    let _ = T::BridgeInterfaceNotification::process_read_result(
                        read_req.read_id,
                        read_req.caller_id.clone().into(),
                        Err(()),
                    );
            
                    process_next_request::<T>();
                }
            }
        };
    };
}

pub fn has_enough_confirmations<T: Config>(
    req: &ActiveRequestData<BlockNumberFor<T>, T::AccountId>,
) -> bool {
    let confirmations = req.confirmation.confirmations.len() as u32;
    match req.request {
        // The sender's confirmation is implicit so we only collect them from other authors:
        Request::Send(_) => util::has_enough_confirmations::<T>(confirmations),
        Request::LowerProof(_) => util::has_supermajority_confirmations::<T>(confirmations),
        Request::ReadContract(_) => util::has_supermajority_confirmations::<T>(confirmations),
    }
}

pub fn complete_lower_proof_request<T: Config>(
    lower_req: &LowerProofRequestData,
    confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
) -> Result<(), Error<T>> {
    let lower_proof = eth::generate_encoded_lower_proof::<T>(lower_req, confirmations);
    let result = T::BridgeInterfaceNotification::process_lower_proof_result(
        lower_req.lower_id,
        lower_req.caller_id.clone().into(),
        Ok(lower_proof),
    );

    if let Err(e) = result {
        log::error!(target: "runtime::eth-bridge", "Lower proof notification failed: {:?}", e);
    }

    // Process any new request from the queue
    request::process_next_request::<T>();

    Ok(())
}

fn set_up_active_lower_proof<T: Config>(req: LowerProofRequestData) -> Result<(), Error<T>> {
    let msg_hash = H256::from(keccak_256(&req.params));

    ActiveRequest::<T>::put(ActiveRequestData::<BlockNumberFor<T>, T::AccountId> {
        request: Request::LowerProof(req),
        confirmation: ActiveConfirmation { msg_hash, confirmations: BoundedVec::default() },
        tx_data: None,
        last_updated: <frame_system::Pallet<T>>::block_number(),
    });

    return Ok(())
}
fn set_up_active_read_contract<T: Config>(req: ReadContractRequestData) -> Result<(), Error<T>> {
    // Encode call data deterministically
    let calldata = eth::abi_encode_function::<T>(
        &req.function_name,
        &util::unbound_params(&req.params),
    )
    .map_err(|_| Error::<T>::FunctionEncodingError)?;

    // ✅ msg_hash includes: read_id + contract_address + calldata + eth_block
    let mut msg: Vec<u8> = Vec::new();
    msg.extend_from_slice(&req.read_id.to_le_bytes());
    msg.extend_from_slice(req.contract_address.as_bytes()); // ✅ NEW
    msg.extend_from_slice(&calldata);

    if let Some(block) = req.eth_block {
        msg.extend_from_slice(&block.to_le_bytes());
    }

    let msg_hash = H256::from_slice(&keccak_256(&msg));

    ActiveRequest::<T>::put(ActiveRequestData::<BlockNumberFor<T>, T::AccountId> {
        request: Request::ReadContract(req),
        confirmation: ActiveConfirmation { msg_hash, confirmations: BoundedVec::default() },
        tx_data: None,
        last_updated: <frame_system::Pallet<T>>::block_number(),
    });

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
