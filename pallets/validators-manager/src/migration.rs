use crate::*;
use frame_support::{
    pallet_prelude::*,
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub struct ValidatorsManagerMigrations<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for ValidatorsManagerMigrations<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "ℹ️  ValidatorsManager migration invoked with current storage version {:?} / onchain {:?}",
            current,
            onchain
        );

        let mut consumed_weight = Weight::zero();

        if onchain < 1 {
            consumed_weight.saturating_accrue(populate_reverse_map::<T>());
        }

        consumed_weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        Ok([0u8; 0].to_vec())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_input: Vec<u8>) -> Result<(), TryRuntimeError> {
        frame_support::ensure!(
            Pallet::<T>::on_chain_storage_version() >= crate::STORAGE_VERSION,
            "ValidatorsManager storage version not bumped"
        );

        // Verify reverse map correctness
        for (eth_key, account_id) in <EthereumPublicKeys<T>>::iter() {
            let reverse = <AccountIdToEthereumKeys<T>>::get(&account_id)
                .ok_or(TryRuntimeError::Other("Missing reverse mapping".into()))?;
            frame_support::ensure!(reverse == eth_key, "Mismatched reverse mapping");
        }

        Ok(())
    }
}

fn populate_reverse_map<T: Config>() -> Weight {
    let mut reads: u64 = 0;
    let mut writes: u64 = 0;

    for (eth_key, account_id) in <EthereumPublicKeys<T>>::iter() {
        match <AccountIdToEthereumKeys<T>>::get(&account_id) {
            Some(existing) => {
                reads += 1;
                if existing != eth_key {
                    <AccountIdToEthereumKeys<T>>::insert(&account_id, eth_key);
                    writes += 1;
                }
            },
            None => {
                <AccountIdToEthereumKeys<T>>::insert(&account_id, eth_key);
                writes += 1;
            },
        }
    }

    // Bump storage version
    crate::STORAGE_VERSION.put::<Pallet<T>>();
    writes += 1;

    T::DbWeight::get().reads_writes(reads, writes)
}
