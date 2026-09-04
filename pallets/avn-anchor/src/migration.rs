use crate::{Config, Pallet, STORAGE_VERSION};
use frame_support::{
    pallet_prelude::PhantomData,
    storage::migration::clear_storage_prefix,
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// Migration v1 -> v2: remove the `ChainData` storage map.
///
/// `ChainData` stored a (chain_id, name) pair keyed by ChainId. The name field is no longer
/// needed at runtime; chain registration now goes through `register_appchain` which stores the
/// name in the asset registry. This migration clears all on-chain entries before the
/// storage type is removed from the pallet.
pub mod v2 {
    use super::*;

    const V2_STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    pub struct Migration<T>(core::marker::PhantomData<T>);

    impl<T: Config> OnRuntimeUpgrade for Migration<T> {
        fn on_runtime_upgrade() -> Weight {
            let current = StorageVersion::get::<Pallet<T>>();
            log::warn!("🚧 🚧 Running avn-anchor v2 migration. Current version: {:?}", current);

            if current != 1 {
                log::warn!(
                    "🚧 🚧 v2 migration skipped: expected storage version 1, found {:?}",
                    current,
                );
                return T::DbWeight::get().reads(1)
            }

            let result = clear_storage_prefix(b"AvnAnchor", b"ChainData", b"", None, None);

            log::info!("✅ v2 migration: removed {} ChainData entries", result.unique,);

            V2_STORAGE_VERSION.put::<Pallet<T>>();

            T::DbWeight::get().reads_writes(1 + result.unique as u64, 1 + result.unique as u64)
        }

        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
            frame_support::ensure!(
                StorageVersion::get::<Pallet<T>>() == 1,
                TryRuntimeError::Other("expected storage version 1 before migration")
            );
            Ok(sp_std::vec![])
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
            frame_support::ensure!(
                StorageVersion::get::<Pallet<T>>() == 2,
                TryRuntimeError::Other("storage version not bumped to 2")
            );
            // Verify ChainData is empty (non-destructive: check if any key still shares the prefix)
            let prefix = frame_support::storage::storage_prefix(b"AvnAnchor", b"ChainData");
            frame_support::ensure!(
                !frame_support::storage::unhashed::contains_prefixed_key(&prefix),
                TryRuntimeError::Other("ChainData storage was not fully cleared")
            );
            Ok(())
        }
    }
}

/// Migration v2 -> v3: `RewardRecord.auto_stake_expiry: u64` becomes `node_serial: u32`.
///
/// The serial of a node whose reward accrued before this upgrade was never captured, so every
/// existing `UnpaidByPeriod` record is translated with [`crate::UNKNOWN_NODE_SERIAL`]. Owner and
/// share are preserved, so the reward is still paid to the right account. `UnpaidByNode`,
/// `SweepCursor` and `PeriodChainReward` are unaffected (their types did not change).
pub mod v3 {
    use super::*;
    use crate::{RewardRecord, UnpaidByPeriod, UNKNOWN_NODE_SERIAL};
    use codec::{Decode, Encode};
    use sp_runtime::Perquintill;

    /// `RewardRecord` as stored under storage version 2.
    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
    pub struct RewardRecordV2<AccountId> {
        pub owner: AccountId,
        pub share: Perquintill,
        pub auto_stake_expiry: u64,
    }

    /// Translate every `UnpaidByPeriod` record to the v3 layout and bump the storage version.
    /// Returns the weight consumed. Callers are responsible for version gating.
    pub fn migrate<T: Config>() -> Weight {
        let mut translated: u64 = 0;
        UnpaidByPeriod::<T>::translate::<RewardRecordV2<T::AccountId>, _>(|_period, _node, old| {
            translated = translated.saturating_add(1);
            Some(RewardRecord {
                owner: old.owner,
                share: old.share,
                node_serial: UNKNOWN_NODE_SERIAL,
            })
        });

        STORAGE_VERSION.put::<Pallet<T>>();

        log::info!(
            "✅ avn-anchor v3 migration: translated {} UnpaidByPeriod record(s) to carry node_serial",
            translated
        );

        // One read + one write per translated record, plus the version write.
        T::DbWeight::get().reads_writes(translated, translated.saturating_add(1))
    }
}

/// Runs every outstanding avn-anchor migration, gated on the on-chain storage version, so it is
/// safe to leave in the runtime's `Executive` migrations tuple across releases.
pub struct AvnAnchorMigrations<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for AvnAnchorMigrations<T> {
    fn on_runtime_upgrade() -> Weight {
        let onchain = Pallet::<T>::on_chain_storage_version();
        let in_code = Pallet::<T>::in_code_storage_version();
        let mut weight = T::DbWeight::get().reads(1);

        if onchain < 2 {
            log::info!(
                "💽 Running avn-anchor v2 migration (in-code {:?} / on-chain {:?})",
                in_code,
                onchain
            );
            weight = weight.saturating_add(v2::Migration::<T>::on_runtime_upgrade());
        }

        if onchain < 3 {
            log::info!(
                "💽 Running avn-anchor v3 migration (in-code {:?} / on-chain {:?})",
                in_code,
                onchain
            );
            weight = weight.saturating_add(v3::migrate::<T>());
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        use codec::Encode;
        // Number of unpaid records must be preserved by the translation.
        let count = crate::UnpaidByPeriod::<T>::iter_keys().count() as u32;
        Ok(count.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        use codec::Decode;
        let before = u32::decode(&mut &state[..])
            .map_err(|_| TryRuntimeError::Other("failed to decode pre_upgrade state"))?;

        frame_support::ensure!(
            Pallet::<T>::on_chain_storage_version() == STORAGE_VERSION,
            TryRuntimeError::Other("avn-anchor storage version not bumped to 3")
        );

        // Every value must decode under the new layout (`iter` skips undecodable values, so compare
        // against the raw key count too).
        let keys = crate::UnpaidByPeriod::<T>::iter_keys().count() as u32;
        let values = crate::UnpaidByPeriod::<T>::iter().count() as u32;
        frame_support::ensure!(
            keys == before,
            TryRuntimeError::Other("UnpaidByPeriod record count changed during migration")
        );
        frame_support::ensure!(
            values == keys,
            TryRuntimeError::Other("some UnpaidByPeriod records do not decode as RewardRecord v3")
        );
        Ok(())
    }
}
