use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::*,
    storage::unhashed,
    storage_alias,
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};
use pallet_avn::vote::VotingSessionData;
use sp_avn_common::IngressCounter;
use sp_std::vec;

use crate::*;

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

mod storage_with_voting {

    use parachain_staking::BlockNumberFor;

    use super::*;

    #[storage_alias]
    pub type VotesRepository<T: Config> = StorageMap<
        Pallet<T>,
        Blake2_128Concat,
        ActionId<<T as frame_system::Config>::AccountId>,
        VotingSessionData<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>,
        ValueQuery,
    >;

    #[storage_alias]
    pub type PendingApprovals<T: Config> = StorageMap<
        Pallet<T>,
        Blake2_128Concat,
        <T as frame_system::Config>::AccountId,
        IngressCounter,
        ValueQuery,
    >;
}

pub struct RemovePalletVoting<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for RemovePalletVoting<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == 0 && current == 1 {
            log::info!(
                "ℹ️  Validator manager data migration invoked with current storage version {:?} / onchain {:?}",
                current,
                onchain
            );
            return remove_storage_items::<T>()
        }

        Weight::zero()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        Ok([0; 32].to_vec())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_input: Vec<u8>) -> Result<(), &'static str> {
        Ok(())
    }
}

pub fn remove_storage_items<T: Config>() -> Weight {
    let mut consumed_weight: Weight = Weight::from_parts(0 as u64, 0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    let approvals_prefix = storage::storage_prefix(b"ValidatorsManager", b"PendingApprovals");
    let mut approvals_key = vec![0u8; 32];
    approvals_key[0..32].copy_from_slice(&approvals_prefix);
    let _ = unhashed::clear_prefix(&approvals_key[0..32], None, None);
    add_weight(0, 1, Weight::from_parts(0 as u64, 0));

    let votes_prefix = storage::storage_prefix(b"ValidatorsManager", b"VotesRepository");
    let mut votes_key = vec![0u8; 32];
    votes_key[0..32].copy_from_slice(&votes_prefix);
    let _ = unhashed::clear_prefix(&votes_key[0..32], None, None);
    add_weight(0, 1, Weight::from_parts(0 as u64, 0));

    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!("✅ Storage Items Drained successfully");

    return consumed_weight + Weight::from_parts(25_000_000_000 as u64, 0)
}
