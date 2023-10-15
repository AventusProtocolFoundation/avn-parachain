use super::*;
use crate::{Config, AVN};
use frame_support::{traits::UnixTime, BoundedVec};
use sp_avn_common::calculate_one_third_quorum;

pub fn time_now<T: Config>() -> u64 {
    <T as pallet::Config>::TimeProvider::now().as_secs()
}

pub fn quorum_reached<T: Config>(entries: u32) -> bool {
    let quorum = calculate_one_third_quorum(AVN::<T>::validators().len() as u32);
    entries >= quorum
}

pub fn has_enough_confirmations<T: Config>(active_tx: &ActiveTransactionData<T>) -> bool {
    let num_confirmations_with_sender = active_tx.data.confirmations.len() as u32 + 1;
    quorum_reached::<T>(num_confirmations_with_sender)
}

pub fn requires_corroboration<T: Config>(
    active_tx: &ActiveTransactionData<T>,
    author: &Author<T>,
) -> bool {
    !active_tx.success_corroborations.contains(&author.account_id) &&
        !active_tx.failure_corroborations.contains(&author.account_id)
}

pub fn bound_params<T>(
    params: &[(Vec<u8>, Vec<u8>)],
) -> Result<
    BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    Error<T>,
> {
    let result: Result<Vec<_>, _> = params
        .iter()
        .map(|(type_vec, value_vec)| {
            let type_bounded = BoundedVec::try_from(type_vec.clone())
                .map_err(|_| Error::<T>::TypeNameLengthExceeded)?;
            let value_bounded = BoundedVec::try_from(value_vec.clone())
                .map_err(|_| Error::<T>::ValueLengthExceeded)?;
            Ok::<_, Error<T>>((type_bounded, value_bounded))
        })
        .collect();

    BoundedVec::<_, ParamsLimit>::try_from(result?).map_err(|_| Error::<T>::ParamsLimitExceeded)
}

pub fn unbound_params(
    params: &BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    params
        .iter()
        .map(|(type_bounded, value_bounded)| {
            (type_bounded.as_slice().to_vec(), value_bounded.as_slice().to_vec())
        })
        .collect()
}
