use frame_support::{
    pallet_prelude::*,
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};

use crate::*;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

mod v2 {
    use super::*;
    use frame_support::storage_alias;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct ActiveEthRange {
        pub range: EthBlockRange,
        pub partition: u16,
        pub event_types_filter: EthBridgeEventsFilter,
    }

    // TODO remove me, not used.
    /// V2 type for [`crate::ActiveEthRange`].
    #[storage_alias]
    pub type ActiveEthereumRange<T: crate::Config> = StorageValue<crate::Pallet<T>, ActiveEthRange>;
}

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
        let mut consumed_weight = Weight::zero();

        if onchain < 2 {
            consumed_weight.saturating_accrue(set_block_range_size::<T>());
        }

        if onchain == 3 && current == 3 {
            consumed_weight.saturating_accrue(migrate_to_v3::<T>());
        }

        consumed_weight
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
    let v2_storage: StorageVersion = StorageVersion::new(2);

    v2_storage.put::<Pallet<T>>();

    // 2 Storage writes
    add_weight(0, 2, Weight::from_parts(0 as u64, 0));

    log::info!("✅ BlockRangeSize set successfully");

    return consumed_weight + Weight::from_parts(25_000 as u64, 0)
}

pub fn migrate_to_v3<T: Config>() -> Weight {
    if let Some(old_range) = v2::ActiveEthereumRange::<T>::take() {
        ActiveEthereumRange::<T>::put(ActiveEthRange {
            range: old_range.range,
            partition: old_range.partition,
            event_types_filter: old_range.event_types_filter,
            additional_events: Default::default(),
        });
        return T::DbWeight::get().reads_writes(1, 1);
    }

    T::DbWeight::get().reads(1)
}
