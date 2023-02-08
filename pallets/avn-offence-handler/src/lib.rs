//! # AVN Offence Handler Pallet
//!
//! This pallet provides functionality to call ethereum transaction to slash the offender.
//! and implements the OnOffenceHandler trait defined in sp_staking.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{dispatch::DispatchResult, traits::Get, weights::Weight};
use frame_system::ensure_root;
pub use pallet::*;
use pallet_avn::{Enforcer, ValidatorRegistrationNotifier};
use pallet_session::{self as session, historical::IdentificationTuple};
use sp_runtime::Perbill;
use sp_staking::{
    offence::{DisableStrategy, OffenceDetails, OnOffenceHandler},
    SessionIndex,
};
use sp_std::prelude::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod benchmarking;

pub mod default_weights;
pub use default_weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, Blake2_128Concat};
    use frame_system::pallet_prelude::*;

    // Public interface of this pallet
    #[pallet::config]
    pub trait Config: frame_system::Config + session::historical::Config {
        /// The overarching event type.
        type Event: From<Event<Self>>
            + Into<<Self as frame_system::Config>::Event>
            + IsType<<Self as frame_system::Config>::Event>;

        /// A trait responsible for punishing malicious validators
        type Enforcer: Enforcer<<Self as session::Config>::ValidatorId>;

        /// Weight information for the extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::event]
    /// This attribute generate the function `deposit_event` to deposit one of this pallet event,
    /// it is optional, it is also possible to provide a custom implementation.
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// One validator has been reported.
        ReportedOffence { offender: T::ValidatorId },
        /// True if slashing is enable, otherwise False
        SlashingConfigurationUpdated { slashing_enabled: bool },
    }

    #[pallet::error]
    pub enum Error<T> {}

    /// A false value means the offence for the validator was not applied successfully.
    #[pallet::storage]
    #[pallet::getter(fn get_reported_offender)]
    pub type ReportedOffenders<T: Config> =
        StorageMap<_, Blake2_128Concat, T::ValidatorId, bool, ValueQuery>;

    /// A flag to control if slashing is enabled
    #[pallet::storage]
    #[pallet::getter(fn can_slash)]
    pub type SlashingEnabled<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(<T as pallet::Config>::WeightInfo::configure_slashing())]
        pub fn configure_slashing(origin: OriginFor<T>, enabled: bool) -> DispatchResult {
            let _sender = ensure_root(origin)?;
            <SlashingEnabled<T>>::put(enabled);

            Self::deposit_event(Event::<T>::SlashingConfigurationUpdated {
                slashing_enabled: enabled,
            });
            Ok(())
        }
    }
}

impl<T: Config> Pallet<T> {
    pub fn setup_for_new_validator(new_validator_id: &<T as session::Config>::ValidatorId) {
        <ReportedOffenders<T>>::remove(new_validator_id);
    }
}

impl<T: Config> OnOffenceHandler<T::AccountId, IdentificationTuple<T>, Weight> for Pallet<T> {
    // This function must not error because failed offences will be retried forever.
    fn on_offence(
        offenders: &[OffenceDetails<T::AccountId, IdentificationTuple<T>>], /* A list containing both current offenders and previous offenders */
        _slash_fraction: &[Perbill],
        _session: SessionIndex,
        _disable_strategy: DisableStrategy,
    ) -> Weight {
        let mut consumed_weight: Weight = 0;
        let mut add_db_reads_writes = |reads, writes| {
            consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        };

        // [Read]: each item is checked by `ReportedOffenders::contains_key`
        add_db_reads_writes(offenders.len() as u64, 0);

        offenders
            .iter()
            .filter(|&detail| !<ReportedOffenders<T>>::contains_key(&detail.offender.0))
            .for_each(|detail| {
                let offender_account_id = &detail.offender.0;
                Self::deposit_event(Event::<T>::ReportedOffence {
                    offender: offender_account_id.clone(),
                });

                let mut result: bool = false;

                // [Read]: can_slash
                add_db_reads_writes(1, 0);
                if Self::can_slash() {
                    result = T::Enforcer::slash_validator(&offender_account_id.clone()).is_ok();
                }

                <ReportedOffenders<T>>::insert(offender_account_id.clone(), result);
                // [Write]: ReportedOffenders
                add_db_reads_writes(0, 1);
            });

        return consumed_weight
    }
}

impl<T: Config> ValidatorRegistrationNotifier<<T as session::Config>::ValidatorId> for Pallet<T> {
    fn on_validator_registration(validator_id: &<T as session::Config>::ValidatorId) {
        Self::setup_for_new_validator(validator_id);
    }
}
