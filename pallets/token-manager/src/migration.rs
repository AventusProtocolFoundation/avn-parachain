use crate::{Config, FailedLowerProofs, LowerNonce, LowerSchedulePeriod, LowerV2Threshold, Pallet};
use frame_support::{
    pallet_prelude::{PhantomData, StorageVersion},
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_avn_common::eth::{LowerParams, PACKED_LOWER_V1_PARAMS_SIZE, PACKED_LOWER_V2_PARAMS_SIZE};

#[cfg(feature = "try-runtime")]
use crate::Vec;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

mod legacy {
    use super::PACKED_LOWER_V1_PARAMS_SIZE;
    pub const V1_SIZE: usize = PACKED_LOWER_V1_PARAMS_SIZE;
    pub type LowerParamsV1 = [u8; V1_SIZE];
}

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

pub fn set_lower_v2_threshold_and_normalise_failed_v1_lower_proofs<T: Config>() -> Weight {
    use legacy::LowerParamsV1;

    let mut consumed_weight: Weight = Weight::from_parts(0 as u64, 0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    log::info!("🚧 🚧 Running migration to set LowerV2Threshold from LowerNonce and normalise FailedLowerProofs");

    let next_lower_id = LowerNonce::<T>::get();
    LowerV2Threshold::<T>::put(next_lower_id);

    // Expand lower params from 76 bytes (V1) to 116 bytes (V2)
    FailedLowerProofs::<T>::translate::<LowerParamsV1, _>(|_lower_id, v1_lower_params| {
        let mut v2_lower_params: LowerParams = [0u8; PACKED_LOWER_V2_PARAMS_SIZE];
        v2_lower_params[..legacy::V1_SIZE].copy_from_slice(&v1_lower_params);
        Some(v2_lower_params)
    });

    // Read: LowerNonce, Write: LowerV2Threshold, STORAGE_VERSION
    add_weight(1, 2, Weight::from_parts(0 as u64, 0));
    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!(
        "✅ LowerV2Threshold successfully set to {:?} and FailedLowerProofs normalised",
        next_lower_id
    );

    // add a bit extra as safety margin for computation
    consumed_weight + Weight::from_parts(50_000_000 as u64, 0)
}

/// Migration to enable staking pallet and set LowerV2Threshold
pub struct SetLowerSchedulePeriod<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for SetLowerSchedulePeriod<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();
        let mut total_weight = Weight::zero();

        if onchain < 1 {
            log::info!(
                "💽 Running Token manager migration with current storage version {:?} / onchain {:?}",
                current,
                onchain
            );
            total_weight += set_lower_schedule_period::<T>();
        }

        if onchain < 2 {
            log::info!(
                "💽 Running LowerV2Threshold migration with current storage version {:?} / onchain {:?}",
                current,
                onchain
            );
            total_weight += set_lower_v2_threshold_and_normalise_failed_v1_lower_proofs::<T>();
        }

        total_weight
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
