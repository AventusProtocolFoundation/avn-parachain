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
    pub type ActiveEthereumRange<T: crate::Config<I>, I: 'static> =
        StorageValue<Pallet<T, I>, ActiveEthRange>;
}

pub struct EthBridgeMigrations<T: Config<I>, I: 'static = ()>(PhantomData<T>, PhantomData<I>);
impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for EthBridgeMigrations<T, I> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T, I>::current_storage_version();
        let onchain = Pallet::<T, I>::on_chain_storage_version();

        log::info!(
            "ℹ️  Eth bridge `BlockRangeSize` invoked with current storage version {:?} / onchain {:?}",
            current,
            onchain
        );
        let mut consumed_weight = Weight::zero();

        if EthBlockRangeSize::<T, I>::get() == 0 {
            consumed_weight.saturating_accrue(set_block_range_size::<T, I>());
        }

        if onchain < 3 && current == 3 {
            consumed_weight.saturating_accrue(migrate_to_v3::<T, I>());
        }

        consumed_weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        Ok([0; 32].to_vec())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_input: Vec<u8>) -> Result<(), TryRuntimeError> {
        frame_support::ensure!(EthBlockRangeSize::<T, I>::get() == DEFAULT_ETH_RANGE, "Block range not set");

        Ok(())
    }
}

pub fn set_block_range_size<T: Config<I>, I: 'static>() -> Weight {
    let mut consumed_weight: Weight = Weight::from_parts(0 as u64, 0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    EthBlockRangeSize::<T, I>::put(DEFAULT_ETH_RANGE);

    // 2 Storage writes
    add_weight(0, 2, Weight::from_parts(0 as u64, 0));

    log::info!("✅ BlockRangeSize set successfully");
    return consumed_weight + Weight::from_parts(25_000 as u64, 0)
}

pub fn migrate_to_v3<T: Config<I>, I: 'static>() -> Weight {
    let mut consumed_weight: Weight = T::DbWeight::get().reads(1);

    if let Some(old_range) = v2::ActiveEthereumRange::<T, I>::take() {
        ActiveEthereumRange::<T, I>::put(ActiveEthRange {
            range: old_range.range,
            partition: old_range.partition,
            event_types_filter: old_range.event_types_filter,
            additional_transactions: Default::default(),
        });
        log::info!("✅ ActiveEthereumRange set successfully");
        consumed_weight += T::DbWeight::get().writes(1);
    }
    STORAGE_VERSION.put::<Pallet<T, I>>();
    consumed_weight += T::DbWeight::get().writes(1);

    consumed_weight
}
