// AvN is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
    weights::WeightInfo, BalanceOf, Config, Delay, Era, EraInfo, Event, Growth, GrowthInfo,
    MinCollatorStake, MinTotalNominatorStake, Pallet, Staked, Total, TotalSelected,
};
use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::{PhantomData, StorageVersion},
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};
use sp_runtime::traits::Zero;

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

pub fn enable_staking<T: Config>() -> Weight {
    let initial_delay: u32 = 2;
    let initial_min_collator_stake = 5_000_000_000_000_000_000_000u128; //5000AVT
    let initial_min_user_stake = 100_000_000_000_000_000_000u128; // 100 AVT
    let intial_era_index = 1u32;
    let initial_growth_period_index = 0u32;
    let current_block_number = frame_system::Pallet::<T>::block_number();

    let mut consumed_weight: Weight = 0;
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    let to_balance = |b| {
        if let Ok(balance) = <BalanceOf<T> as TryFrom<u128>>::try_from(b).or_else(|e| Err(e)) {
            return Ok(balance)
        }

        log::error!("💔 Error converting amount to balance: {:?}", b);
        return Err(())
    };

    log::info!("🚧 🚧 Running migration to enable parachain staking");

    // Since we are hardcoding the amount, this will probably never fail, but we want to be 100%
    // sure.
    if let Err(_) = to_balance(initial_min_collator_stake) {
        log::error!("Exiting migration script due to previous errors");
        return consumed_weight
    }

    if let Err(_) = to_balance(initial_min_user_stake) {
        log::error!("Exiting migration script due to previous errors");
        return consumed_weight
    }

    let initial_min_collator_stake_balance =
        to_balance(initial_min_collator_stake).expect("Asserted");
    let mut candidate_count = 0u32;

    //Reads: [validators]
    add_weight(1, 0, 0);

    // Initialize the candidates
    for validator in pallet_avn::Pallet::<T>::validators() {
        //Reads: [get_collator_stakable_free_balance]
        add_weight(1, 0, 0);

        assert!(
            <Pallet<T>>::get_collator_stakable_free_balance(&validator.account_id) >=
                initial_min_collator_stake_balance,
            "Account does not have enough balance to bond as a candidate."
        );

        candidate_count = candidate_count.saturating_add(1u32);

        if let Err(error) = <Pallet<T>>::join_candidates(
            T::Origin::from(Some(validator.account_id).into()),
            initial_min_collator_stake_balance,
            candidate_count,
        ) {
            log::error!("💔 Join candidates failed in genesis with error {:?}", error);
            continue
        } else {
            candidate_count = candidate_count.saturating_add(1u32);
        }

        add_weight(0, 0, <T as Config>::WeightInfo::join_candidates(candidate_count));
    }

    log::info!("    - Converted {:?} collator as stakers", candidate_count);

    // Validate and set delay
    assert!(initial_delay > 0, "Delay must be greater than 0.");

    //Write: [Delay]
    add_weight(0, 1, 0);
    <Delay<T>>::put(initial_delay);

    // Set min staking values
    //Write: [MinCollatorStake, MinTotalNominatorStake, TotalSelected]
    add_weight(0, 3, 0);
    <MinCollatorStake<T>>::put(initial_min_collator_stake_balance);
    <MinTotalNominatorStake<T>>::put(to_balance(initial_min_user_stake).expect("Asserted"));
    <TotalSelected<T>>::put(T::MinSelectedCandidates::get());

    // Choose top TotalSelected collator candidates
    let (collator_count, _, total_staked) = <Pallet<T>>::select_top_candidates(intial_era_index);
    // TODO: benchmark this and add to weight
    // add_weight(0, 0, <T as
    // pallet_staking::Config>::WeightInfo::select_top_candidates(candidate_count));

    // Calculate the first era info.
    let era: EraInfo<T::BlockNumber> =
        EraInfo::new(intial_era_index, current_block_number.into(), T::MinBlocksPerEra::get() + 2);

    //Write: [Era, Staked, Growth]
    add_weight(0, 3, 0);
    // Set the first era info.
    <Era<T>>::put(era);
    // Snapshot total stake
    <Staked<T>>::insert(intial_era_index, <Total<T>>::get());
    // Set the first GrowthInfo
    <Growth<T>>::insert(initial_growth_period_index, GrowthInfo::new(1u32));

    <Pallet<T>>::deposit_event(Event::NewEra {
        starting_block: current_block_number,
        era: intial_era_index,
        selected_collators_number: collator_count,
        total_balance: total_staked,
    });

    //Write: [STORAGE_VERSION]
    add_weight(0, 1, 0);
    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!("✅ Migration completed successfully");

    // add a bit extra as safety margin for computation
    return consumed_weight + 25_000_000_000
}

/// Migration to enable staking pallet
pub struct EnableStaking<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for EnableStaking<T> {
    fn on_runtime_upgrade() -> Weight {
        let onchain_version = Pallet::<T>::on_chain_storage_version();

        if onchain_version < 1 {
            return enable_staking::<T>()
        }

        Weight::zero()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<(), &'static str> {
        Ok(())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade() -> Result<(), &'static str> {
        Ok(())
    }
}
