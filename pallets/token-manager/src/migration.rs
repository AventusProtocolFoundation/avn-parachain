use crate::{Config, LowerSchedulePeriod, Pallet};
use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::{PhantomData, StorageVersion},
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;

#[cfg(feature = "try-runtime")]
use crate::Vec;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

pub fn set_lower_schedule_period<T: Config>() -> Weight {
    let default_lower_schedule_period: BlockNumberFor<T> = 3275u32.into(); // ~ 12 hrs
    let mut consumed_weight: Weight = Weight::from_parts(0 as u64, 0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    log::info!("🚧 🚧 Running migration to set lower schedule period");

    <LowerSchedulePeriod<T>>::put(default_lower_schedule_period);

    //Write: [LowerSchedulePeriod, STORAGE_VERSION]
    add_weight(0, 2, Weight::from_parts(0 as u64, 0));
    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!("✅ Lower schedule period successfully");

    // add a bit extra as safety margin for computation
    return consumed_weight + Weight::from_parts(25_000_000 as u64, 0)
}

/// Migration to enable staking pallet
pub struct SetLowerSchedulePeriod<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for SetLowerSchedulePeriod<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain < 1 {
            log::info!(
                "💽 Running Token manager migration with current storage version {:?} / onchain {:?}",
                current,
                onchain
            );
            return set_lower_schedule_period::<T>()
        }

        Weight::zero()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        use codec::Encode;

        Ok(<LowerSchedulePeriod<T>>::get().encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(input: Vec<u8>) -> Result<(), TryRuntimeError> {
        use codec::Decode;
        use sp_runtime::traits::Zero;

        let initial_lower_schedule_period: BlockNumberFor<T> =
            Decode::decode(&mut input.as_slice()).expect("Initial lower schedule is invalid");
        if initial_lower_schedule_period == BlockNumberFor::<T>::zero() {
            assert_eq!(initial_lower_schedule_period, 3275u32.into());
            log::info!(
                "💽 lower_schedule_period updated successfully to {:?}",
                <LowerSchedulePeriod<T>>::get()
            );
        } else {
            log::info!(
                "💽 lower_schedule_period was not updated because it had a non zero value: {:?}",
                initial_lower_schedule_period
            );
        }

        Ok(())
    }
}
