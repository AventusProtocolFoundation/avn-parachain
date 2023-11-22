use crate::{Config, Pallet, LowerSchedulePeriod};
use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::{PhantomData, StorageVersion},
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);


pub fn set_lower_schedule_period<T: Config>() -> Weight {
    let default_lower_schedule_period: T::BlockNumber = 3275u32.into(); // ~ 12 hrs
    let mut consumed_weight: Weight = Weight::from_ref_time(0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    log::info!("🚧 🚧 Running migration to set lower schedule period");

    <LowerSchedulePeriod<T>>::put(default_lower_schedule_period);

    //Write: [LowerSchedulePeriod, STORAGE_VERSION]
    add_weight(0, 2, Weight::from_ref_time(0));
    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!("✅ Lower schedule period successfully");

    // add a bit extra as safety margin for computation
    return consumed_weight + Weight::from_ref_time(25_000_000)
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
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        Ok(<LowerSchedulePeriod<T>>::get().encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(input: Vec<u8>) -> Result<(), &'static str> {
        let initial_lower_schedule_period: T::BlockNumber = Decode::decode(&mut input.as_slice()).expect("Initial lower schedule is invalid");
        if initial_lower_schedule_period == T::BlockNumber::zero()  {
            assert_eq!(initial_lower_schedule_period, 3600u32.into());
            log::info!("💽 lower_schedule_period updated successfully to {:?}", <LowerSchedulePeriod<T>>::get());
        } else {
            log::info!("💽 lower_schedule_period was not updated because it had a non zero value: {:?}", initial_lower_schedule_period);
        }

        Ok(())
    }
}
