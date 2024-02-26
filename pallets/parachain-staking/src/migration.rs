// AvN is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
    avn::vote::VotingSessionData, BalanceOf, BoundedVec, Clone, CollatorScore, Config, ConstU32,
    Decode, Encode, EthereumTransactionId, Growth, GrowthInfo, GrowthPeriodIndex, IngressCounter,
    MaxEncodedLen, Pallet, RewardPoint, RuntimeDebug, TypeInfo,
};

use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::*,
    storage::unhashed,
    storage_alias,
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};
use sp_std::prelude::*;

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct OldGrowthInfo<AccountId, Balance> {
    pub number_of_accumulations: GrowthPeriodIndex,
    pub total_stake_accumulated: Balance,
    pub total_staker_reward: Balance,
    pub total_points: RewardPoint,
    pub collator_scores: BoundedVec<CollatorScore<AccountId>, ConstU32<10000>>,
    pub added_by: Option<AccountId>,
    pub tx_id: Option<EthereumTransactionId>,
    pub triggered: Option<bool>,
}

/// The original data layout of the storage with voting logic included
mod storage_with_voting {
    use frame_system::pallet_prelude::BlockNumberFor;

    use super::*;

    #[derive(
        Encode, Decode, Default, Clone, Copy, PartialEq, Debug, Eq, TypeInfo, MaxEncodedLen,
    )]
    pub struct GrowthId {
        pub period: GrowthPeriodIndex,
        pub ingress_counter: IngressCounter,
    }

    #[storage_alias]
    pub type VotesRepository<T: Config> = StorageMap<
        Pallet<T>,
        Blake2_128Concat,
        GrowthId,
        VotingSessionData<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>,
        ValueQuery,
    >;

    #[storage_alias]
    pub type TotalIngresses<T: Config> = StorageValue<Pallet<T>, IngressCounter, ValueQuery>;

    #[storage_alias]
    pub type VotingPeriod<T: Config> =
        StorageValue<Pallet<T>, frame_system::pallet_prelude::BlockNumberFor<T>, ValueQuery>;
}

pub fn enable_eth_bridge_wire_up<T: Config>() -> Weight {
    let mut consumed_weight: Weight = Weight::from_parts(0 as u64, 0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    log::info!("🚧 🚧 Running migration to wire up eth bridge with auto growth");

    // Delete storage items we don't need
    storage_with_voting::TotalIngresses::<T>::kill();
    storage_with_voting::VotingPeriod::<T>::kill();

    let votes_repo_prefix = storage::storage_prefix(b"ParachainStaking", b"VotesRepository");
    let mut key = vec![0u8; 32];
    key[0..32].copy_from_slice(&votes_repo_prefix);
    let _ = unhashed::clear_prefix(&key[0..32], None, None);

    // Remove unused `added_by` field
    Growth::<T>::translate::<OldGrowthInfo<T::AccountId, BalanceOf<T>>, _>(
        |_period, growth_info| {
            add_weight(1, 1, Weight::from_parts(0 as u64, 0));

            let mut new_growth_info = GrowthInfo::new(growth_info.number_of_accumulations);
            new_growth_info.total_stake_accumulated = growth_info.total_stake_accumulated;
            new_growth_info.total_staker_reward = growth_info.total_staker_reward;
            new_growth_info.total_points = growth_info.total_points;
            new_growth_info.collator_scores = growth_info.collator_scores;
            new_growth_info.tx_id = growth_info.tx_id;
            new_growth_info.triggered = growth_info.triggered;

            Some(new_growth_info)
        },
    );

    //Write: [STORAGE_VERSION]
    add_weight(0, 1, Weight::from_parts(0 as u64, 0));
    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!("✅ Eth bridge wire up migration completed successfully");

    // add a bit extra as safety margin for computation
    return consumed_weight + Weight::from_parts(25_000_000_000 as u64, 0)
}

/// Migration to enable staking pallet
pub struct EnableEthBridgeWireUp<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for EnableEthBridgeWireUp<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == 2 && current == 3 {
            log::info!(
                "ℹ️  Parachain staking data migration invoked with current storage version {:?} / onchain {:?}",
                current,
                onchain
            );
            return enable_eth_bridge_wire_up::<T>()
        }

        Weight::zero()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == 2 && current == 3 {
            assert_eq!(true, storage_with_voting::VotingPeriod::<T>::exists());
        }

        Ok(vec![])
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_input: Vec<u8>) -> Result<(), &'static str> {
        let current_version = Pallet::<T>::current_storage_version();
        let onchain_version = Pallet::<T>::on_chain_storage_version();

        frame_support::ensure!(current_version == 3, "must_upgrade");
        assert_eq!(
            current_version, onchain_version,
            "after migration, the current_version and onchain_version should be the same"
        );

        assert_eq!(false, storage_with_voting::TotalIngresses::<T>::exists());
        assert_eq!(false, storage_with_voting::VotingPeriod::<T>::exists());

        let votes_repo_prefix = storage::storage_prefix(b"ParachainStaking", b"VotesRepository");
        let map = unhashed::get_raw(&votes_repo_prefix);
        assert_eq!(None, map);

        Ok(())
    }
}
