use super::pallet::*;
use crate::{default_weights::WeightInfo, BalanceOf, PALLET_ID};
use frame_support::{
    pallet_prelude::Weight,
    traits::{Currency, ReservableCurrency},
    PalletId,
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_avn::BridgeInterface;
use sp_avn_common::BridgeContractMethod;
use sp_core::U256;
use sp_runtime::{
    traits::{AccountIdConversion, Saturating, Zero},
    DispatchError,
};

impl<T: Config> Pallet<T> {
    pub(crate) fn is_burn_due(now: BlockNumberFor<T>) -> bool {
        now >= NextBurnAt::<T>::get()
    }

    pub(crate) fn burn_pot_account() -> T::AccountId {
        PalletId(sp_avn_common::BURN_POT_ID).into_account_truncating()
    }

    pub(crate) fn burn(now: BlockNumberFor<T>) -> Weight {
        let burn_pot = Self::burn_pot_account();
        let amount: BalanceOf<T> = T::Currency::free_balance(&burn_pot);

        NextBurnAt::<T>::put(now.saturating_add(BlockNumberFor::<T>::from(BurnPeriod::<T>::get())));

        if !amount.is_zero() {
            let _ = Self::publish_burn_tokens_on_t1(amount);
            Self::deposit_event(Event::<T>::BurnRequested { amount });
            return <T as Config>::WeightInfo::on_initialize_burn_due_and_pot_has_funds_to_burn();
        }

        <T as Config>::WeightInfo::on_initialize_burn_due_but_pot_empty()
    }

    pub(crate) fn publish_burn_tokens_on_t1(amount: BalanceOf<T>) -> Result<(), DispatchError> {
        let burn_pot = Self::burn_pot_account();

        // lock funds until Ethereum burn is confirmed
        T::Currency::reserve(&burn_pot, amount).map_err(|_| Error::<T>::ErrorLockingTokens)?;

        // TODO: convert amount to the right type to match the contract
        let test_amount = U256::from(100);

        let function_name: &[u8] = BridgeContractMethod::BurnTokens.as_bytes();
        let params = vec![(b"uint256".to_vec(), format!("{}", test_amount).into_bytes())];

        match T::BridgeInterface::publish(function_name, &params, PALLET_ID.to_vec()) {
            Ok(tx_id) => {
                PendingBurnSubmission::<T>::insert(tx_id, amount);
                Ok(())
            },
            Err(_) => {
                T::Currency::unreserve(&burn_pot, amount);
                Err(Error::<T>::FailedToSubmitBurnRequest.into())
            },
        }
    }
}
