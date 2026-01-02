use super::*;
use frame_support::{
    traits::{Currency, ExistenceRequirement},
    weights::Weight,
    PalletId,
};
use sp_runtime::traits::{AccountIdConversion, Saturating, Zero};

use sp_runtime::DispatchError;

pub trait TreasuryManager<T: pallet::Config> {
    /// Funds the treasury and immediately moves excess to burn pot.
    fn fund_treasury(from: T::AccountId, amount: crate::BalanceOf<T>)
        -> Result<(), DispatchError>;
}

impl<T: pallet::Config> pallet::Pallet<T> {
    /// How much is above the threshold 
    pub fn treasury_excess() -> crate::BalanceOf<T> {
        let total_supply = TotalSupply::<T>::get();
        if total_supply.is_zero() {
            return Zero::zero();
        }

        let treasury = Self::compute_treasury_account_id();
        let treasury_balance = T::Currency::free_balance(&treasury);

        let threshold = T::TreasuryBurnThreshold::get() * total_supply;
        treasury_balance.saturating_sub(threshold)
    }

    /// Moves the excess from treasury to the burn pot
    pub fn move_treasury_excess_if_required() {
        let excess = Self::treasury_excess();
        if excess.is_zero() {
            return;
        }

        let treasury = Self::compute_treasury_account_id();
        let burn_pot = Self::burn_pot_account();

        if T::Currency::transfer(&treasury, &burn_pot, excess, ExistenceRequirement::KeepAlive)
            .is_ok()
        {
            Self::deposit_event(pallet::Event::<T>::TreasuryExcessSentToBurnPot { amount: excess });
        }
    }
}

impl<T: pallet::Config> TreasuryManager<T> for pallet::Pallet<T> {
    fn fund_treasury(
        from: T::AccountId,
        amount: crate::BalanceOf<T>,
    ) -> Result<(), DispatchError> {
        let treasury = Self::compute_treasury_account_id();

        T::Currency::transfer(&from, &treasury, amount, ExistenceRequirement::KeepAlive)?;

        Self::deposit_event(pallet::Event::<T>::TreasuryFunded { from, amount });

        Self::move_treasury_excess_if_required();

        Ok(())
    }
}
