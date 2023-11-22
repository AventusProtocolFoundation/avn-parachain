use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::*,
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight, storage_alias,
};
use sp_avn_common::IngressCounter;
use pallet_avn::vote::VotingSessionData;

use crate::{Config, Pallet, ActionId};


pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

mod storage_with_voting {

    use super::*;

    #[storage_alias]
    pub type VotesRepository<T: Config> = StorageMap<
        Pallet<T>,
        Blake2_128Concat,
        ActionId<<T as frame_system::Config>::AccountId>,
        VotingSessionData<<T as frame_system::Config>::AccountId, <T as frame_system::Config>::BlockNumber>,
        ValueQuery,
    >;

    #[storage_alias]
    pub type PendingApprovals<T: Config> =
        StorageMap<Pallet<T>, Blake2_128Concat, <T as frame_system::Config>::AccountId, IngressCounter, ValueQuery>;

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
        
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_input: Vec<u8>) -> Result<(), &'static str> {
        
    }
}

pub fn remove_storage_items<T: Config>() -> Weight {
    let mut consumed_weight: Weight = Weight::from_ref_time(0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    storage_with_voting::PendingApprovals::<T>::drain();
    add_weight(0,1,Weight::from_ref_time(0));
    storage_with_voting::VotesRepository::<T>::drain(); 
    add_weight(0,1,Weight::from_ref_time(0));

    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!("✅ Storage Items Drained successfully");

    return consumed_weight + Weight::from_ref_time(25_000_000_000)
}