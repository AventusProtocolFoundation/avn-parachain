// Add this in migrations.rs
use super::*;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_runtime::DispatchError;

/// Migration to erase old checkpoints and prepare for new checkpoint format
pub struct MigrateToV2<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
        // Check if we need migration by trying to read an old checkpoint
        let any_old_checkpoint_exists = Checkpoints::<T>::iter().next().is_some();
        log::info!(
            target: "runtime::avn-anchor",
            "🔄 Starting migration to v2: Old checkpoints exist: {:?}",
            any_old_checkpoint_exists
        );

        Ok(Vec::new())
    }

    fn on_runtime_upgrade() -> Weight {
        let mut reads = 0u64;
        let mut writes = 0u64;

        // Clear old checkpoints storage
        Checkpoints::<T>::clear(1000000000, None);
        writes += 1;

        // Iterate over ChainData instead of ChainHandlers
        for (chain_id, _) in ChainData::<T>::iter() {
            NextCheckpointId::<T>::insert(chain_id, 0);
            reads += 1;
            writes += 1;
        }

        // Clear the OriginIdToCheckpoint storage (in case it was somehow populated)
        OriginIdToCheckpoint::<T>::clear(1000000000,None);
        writes += 1;

        log::info!(
            target: "runtime::avn-anchor",
            "✅ Migration to v2 completed successfully"
        );

        T::DbWeight::get().reads(reads) + T::DbWeight::get().writes(writes)
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
        // Verify storages are empty
        ensure!(
            Checkpoints::<T>::iter().next().is_none(),
            DispatchError::Other("Checkpoints should be empty")
        );
        ensure!(
            OriginIdToCheckpoint::<T>::iter().next().is_none(),
            DispatchError::Other("Origin ID mapping should be empty")
        );

        // Verify all checkpoint IDs are reset to 0
        for (chain_id, _) in ChainData::<T>::iter() {
            ensure!(
                NextCheckpointId::<T>::get(chain_id) == 0,
                DispatchError::Other("Next checkpoint ID should be 0")
            );
        }

        log::info!(
            target: "runtime::avn-anchor",
            "✅ Post migration checks passed successfully"
        );

        Ok(())
    }
}
