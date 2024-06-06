use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::*,
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};

use sp_std::vec;

use crate::*;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub struct SetBlockRangeSize<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for SetBlockRangeSize<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "ℹ️  Eth bridge `BlockRangeSize` invoked with current storage version {:?} / onchain {:?}",
            current,
            onchain
        );

        if onchain < 2 && current == 2 {
            return set_block_range_size::<T>()
        }

        Weight::zero()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        Ok([0; 32].to_vec())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_input: Vec<u8>) -> Result<(), TryRuntimeError> {
        frame_support::ensure!(EthBlockRangeSize::<T>::get() == 20u32, "Block range not set");

        Ok(())
    }
}

pub fn set_block_range_size<T: Config>() -> Weight {
    let mut consumed_weight: Weight = Weight::from_parts(0 as u64, 0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    EthBlockRangeSize::<T>::put(20u32);
    STORAGE_VERSION.put::<Pallet<T>>();

    // 2 Storage writes
    add_weight(0, 2, Weight::from_parts(0 as u64, 0));

    log::info!("✅ BlockRangeSize set successfully");

    return consumed_weight + Weight::from_parts(25_000 as u64, 0)
}
