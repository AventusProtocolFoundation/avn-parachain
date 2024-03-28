use super::*;
use crate::{offence::create_and_report_corroboration_offence, util::unbound_params, Config};
use avn::OperationType;
use frame_support::BoundedVec;

pub fn is_active_request<T: Config>(id: EthereumId) -> bool {
    ActiveRequest::<T>::get().map_or(false, |r| r.request.id_matches(&id))
}

fn complete_transaction<T: Config>(
    mut tx: ActiveTransactionData<T>,
    success: bool,
) -> Result<(), Error<T>> {
    // Alert the originating pallet:
    T::BridgeInterfaceNotification::process_result(
        tx.request.tx_id,
        tx.request.caller_id.into(),
        success,
    )
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
        tx.request.tx_id,
        TransactionData {
            function_name: tx.request.function_name,
            params: tx.data.eth_tx_params,
            sender: tx.data.sender,
            eth_tx_hash: tx.data.eth_tx_hash,
            tx_succeeded: tx.data.tx_succeeded,
        },
    );

    // Process any new request from the queue
    request::process_next_request::<T>();

    Ok(())
}

pub fn finalize_state<T: Config>(
    tx: ActiveTransactionData<T>,
    success: bool,
) -> Result<(), Error<T>> {
    // if the transaction failed and the tx hash is missing or pointing to a different transaction,
    // replay transaction
    if !success &&
        util::has_enough_corroborations::<T>(tx.data.invalid_tx_hash_corroborations.len())
    {
        // raise an offence on the "sender" because the tx_hash they provided was invalid
        return Ok(replay_send_request(tx)?)
    }

    Ok(complete_transaction::<T>(tx, success)?)
}

pub fn set_up_active_tx<T: Config>(req: SendRequestData) -> Result<(), Error<T>> {
    let expiry = util::time_now::<T>() + EthTxLifetimeSecs::<T>::get();
    let extended_params = req.extend_params(expiry)?;
    let msg_hash = generate_msg_hash::<T>(&extended_params)?;
    let new_sender = assign_sender()?;

    ActiveRequest::<T>::put(ActiveRequestData {
        request: Request::Send(req.clone()),
        confirmation: ActiveConfirmation { msg_hash, confirmations: BoundedVec::default() },
        tx_data: Some(ActiveEthTransaction {
            function_name: req.function_name.clone(),
            eth_tx_params: extended_params.clone(),
            expiry,
            eth_tx_hash: H256::zero(),
            sender: assign_sender()?,
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

pub fn replay_send_request<T: Config>(mut tx: ActiveTransactionData<T>) -> Result<(), Error<T>> {
    <crate::Pallet<T>>::deposit_event(Event::<T>::ActiveRequestRetried {
        function_name: tx.request.function_name.clone(),
        params: tx.request.params.clone(),
        caller_id: tx.request.caller_id.clone(),
    });

    tx.request.tx_id = use_next_tx_id::<T>();
    return Ok(set_up_active_tx(tx.request)?)
}

pub fn use_next_tx_id<T: Config>() -> u32 {
    let tx_id = NextTxId::<T>::get();
    NextTxId::<T>::put(tx_id + 1);
    tx_id
}

fn generate_msg_hash<T: pallet::Config>(
    params: &BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
) -> Result<H256, Error<T>> {
    let params = unbound_params(params);
    let tokens: Result<Vec<_>, _> = params
        .iter()
        .map(|(type_bytes, value_bytes)| {
            let param_type =
                eth::to_param_type(type_bytes).ok_or_else(|| Error::<T>::MsgHashError)?;
            eth::to_token_type(&param_type, value_bytes)
        })
        .collect();

    let encoded = ethabi::encode(&tokens?);
    let msg_hash = keccak_256(&encoded);

    Ok(H256::from(msg_hash))
}

fn assign_sender<T: Config>() -> Result<T::AccountId, Error<T>> {
    match AVN::<T>::advance_primary_validator(OperationType::Ethereum) {
        Ok(primary_validator) => {
            let sender = primary_validator;
            Ok(sender)
        },
        Err(_) => Err(Error::<T>::ErrorAssigningSender),
    }
}
