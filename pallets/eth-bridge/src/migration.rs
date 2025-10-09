use frame_support::{
    pallet_prelude::*,
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};

use crate::*;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub const V3_STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

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

mod v3 {
    use super::*;
    /// Version 3 of ActiveRequestData, used in migration from v3 to v4.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct ActiveRequestDataV3<BlockNumber, AccountId> {
        pub request: Request,
        pub confirmation: ActiveConfirmation,
        pub tx_data: Option<ActiveEthTransactionV3<AccountId>>,
        pub last_updated: BlockNumber,
    }

    /// Version 3 of ActiveEthTransaction, used in migration from v3 to v4.
    #[derive(Encode, Decode, Debug, Clone, PartialEq, Eq, Default, TypeInfo, MaxEncodedLen)]
    pub struct ActiveEthTransactionV3<AccountId> {
        pub function_name: BoundedVec<u8, FunctionLimit>,
        pub eth_tx_params:
            BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
        pub sender: AccountId,
        pub expiry: u64,
        pub eth_tx_hash: H256,
        pub success_corroborations: BoundedVec<AccountId, ConfirmationsLimit>,
        pub failure_corroborations: BoundedVec<AccountId, ConfirmationsLimit>,
        pub valid_tx_hash_corroborations: BoundedVec<AccountId, ConfirmationsLimit>,
        pub invalid_tx_hash_corroborations: BoundedVec<AccountId, ConfirmationsLimit>,
        pub tx_succeeded: bool,
    }
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

        if onchain < 3 {
            consumed_weight.saturating_accrue(migrate_to_v3::<T, I>());
        }

        if onchain < 4 && current == 4 {
            consumed_weight.saturating_accrue(migrate_to_v4::<T, I>());
        }

        consumed_weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        Ok([0; 32].to_vec())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_input: Vec<u8>) -> Result<(), TryRuntimeError> {
        frame_support::ensure!(
            EthBlockRangeSize::<T, I>::get() == DEFAULT_ETH_RANGE,
            "Block range not set"
        );

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
    V3_STORAGE_VERSION.put::<Pallet<T, I>>();
    consumed_weight += T::DbWeight::get().writes(1);

    consumed_weight
}

pub fn migrate_to_v4<T: Config<I>, I: 'static>() -> Weight {
    let mut consumed_weight: Weight = T::DbWeight::get().reads(1);

    log::info!("🔄 Starting ActiveRequest migration from v3 to v4");

    let translate = |old: v3::ActiveRequestDataV3<BlockNumberFor<T>, T::AccountId>| -> ActiveRequestData<BlockNumberFor<T>, T::AccountId> {
        let tx_data: Option<ActiveEthTransaction<T::AccountId>> = match old.tx_data {
            Some(data) => Some(ActiveEthTransaction {
                function_name: data.function_name,
                eth_tx_params: data.eth_tx_params,
                sender: data.sender,
                expiry: data.expiry,
                eth_tx_hash: data.eth_tx_hash,
                success_corroborations: data.success_corroborations,
                failure_corroborations: data.failure_corroborations,
                valid_tx_hash_corroborations: data.valid_tx_hash_corroborations,
                invalid_tx_hash_corroborations: data.invalid_tx_hash_corroborations,
                tx_succeeded: data.tx_succeeded,
                replay_attempt: 0, // New field, defaulting to 0
            }),
            None => None,
        };
        let new = ActiveRequestData {
            request: old.request,
            confirmation: old.confirmation,
            last_updated: old.last_updated,
            tx_data,
        };
        log::info!("✅ ActiveRequest migration has been successful");
        new
    };

    if ActiveRequest::<T, I>::translate(|pre| pre.map(translate)).is_err() {
        log::error!(
            "unexpected error when performing translation of the ActiveRequest type \
            during storage upgrade to v4"
        );
    }
    consumed_weight += T::DbWeight::get().writes(1);
    STORAGE_VERSION.put::<Pallet<T, I>>();
    consumed_weight += T::DbWeight::get().writes(1);

    consumed_weight
}
