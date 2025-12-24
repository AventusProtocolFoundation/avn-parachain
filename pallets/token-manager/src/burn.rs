use super::pallet::*;
use crate::{default_weights::WeightInfo, BalanceOf, PALLET_ID};
use frame_support::{pallet_prelude::Weight, traits::Currency, PalletId};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_avn::BridgeInterface;
use scale_info::prelude::vec;
use sp_avn_common::BridgeContractMethod;
use sp_core::U256;
use sp_runtime::{
    traits::{AccountIdConversion, Saturating, Zero},
    DispatchError,
};

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::{format, vec::Vec};

impl<T: Config> Pallet<T> {
    pub(crate) fn is_burn_due(now: BlockNumberFor<T>) -> bool {
        now >= NextBurnAt::<T>::get()
    }

    pub(crate) fn burn_pot_account() -> T::AccountId {
        PalletId(sp_avn_common::BURN_POT_ID).into_account_truncating()
    }

    pub(crate) fn burn_if_required(now: BlockNumberFor<T>) -> Weight {
        let burn_pot = Self::burn_pot_account();
        let amount: BalanceOf<T> = T::Currency::free_balance(&burn_pot);

        NextBurnAt::<T>::put(
            now.saturating_add(BlockNumberFor::<T>::from(BurnRefreshRange::<T>::get())),
        );

        if !amount.is_zero() {
            Self::deposit_event(Event::<T>::BurnedFromPot { amount });
            let _ = Self::burn_tokens(amount);
            return <T as Config>::WeightInfo::on_initialize_burn_due_and_pot_has_funds_to_burn();
        }

        <T as Config>::WeightInfo::on_initialize_burn_due_but_pot_empty()
    }

    pub(crate) fn burn_tokens(_amount: BalanceOf<T>) -> Result<(), DispatchError> {
        // TODO: convert amount to the right type to match the contract
        let test_amount = U256::from(100);

        let function_name: &[u8] = BridgeContractMethod::BurnTokens.as_bytes();
        let params = vec![(b"uint256".to_vec(), format!("{}", test_amount).into_bytes())];

        let tx_id = T::BridgeInterface::publish(function_name, &params, PALLET_ID.to_vec())
            .map_err(|e| DispatchError::Other(e.into()))?;

        PendingBurnSubmission::<T>::insert(tx_id, test_amount);
        Ok(())
    }
}
