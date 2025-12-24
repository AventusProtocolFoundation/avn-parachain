use super::*;
use frame_support::{
    ensure,
    traits::{Currency, ExistenceRequirement},
    PalletId,
};
use sp_runtime::{
    traits::{AccountIdConversion, Saturating, Zero},
    DispatchError,
};

pub trait TreasuryManager<T: pallet::Config> {
    /// Funds the treasury and then requests totalSupply from Ethereum.
    fn fund_treasury(from: &T::AccountId, amount: crate::BalanceOf<T>)
        -> Result<(), DispatchError>;
}

impl<T: pallet::Config> pallet::Pallet<T> {
    pub fn treasury_pot_account() -> T::AccountId {
        PalletId(sp_avn_common::TREASURY_POT_ID).into_account_truncating()
    }

    pub fn request_total_supply() -> Result<u32, DispatchError> {
        ensure!(
            PendingTotalSupplyRead::<T>::get().is_none(),
            DispatchError::Other("TotalSupply read already pending")
        );

        let token_contract = AVTTokenContract::<T>::get();

        let function_name = b"totalSupply";
        let params: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();

        let caller_id = PALLET_ID.to_vec();

        let read_id = T::BridgeInterface::read_contract(
            token_contract,
            function_name,
            &params,
            caller_id,
            None,
        )?;

        PendingTotalSupplyRead::<T>::put(read_id);

        Ok(read_id)
    }

    pub fn move_treasury_excess_if_required_with_total_supply(total_supply: crate::BalanceOf<T>) {
        if total_supply.is_zero() {
            return;
        }

        let treasury = Self::treasury_pot_account();
        let treasury_balance = T::Currency::free_balance(&treasury);

        let threshold = T::TreasuryBurnThreshold::get() * total_supply;
        let excess = treasury_balance.saturating_sub(threshold);

        if excess.is_zero() {
            return;
        }

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
        from: &T::AccountId,
        amount: crate::BalanceOf<T>,
    ) -> Result<(), DispatchError> {
        let treasury = Self::treasury_pot_account();

        T::Currency::transfer(from, &treasury, amount, ExistenceRequirement::KeepAlive)?;

        Self::deposit_event(pallet::Event::<T>::TreasuryFunded { from: from.clone(), amount });

        let _ = Self::request_total_supply();

        Ok(())
    }
}
