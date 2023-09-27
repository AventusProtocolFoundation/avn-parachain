// AvN is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
    BalanceOf, Config, Growth, GrowthInfo,
    Pallet, ProcessedGrowthPeriods, LastTriggeredGrowthPeriod
};
use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::{PhantomData, StorageVersion},
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

pub fn enable_automatic_growth<T: Config>() -> Weight {        
    let mut consumed_weight: Weight = Weight::from_ref_time(0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };
    
    log::info!("🚧 🚧 Running migration to enable automatic growths");
        
    // We only have a handfull of these so performance is not an issue here.
    let mut processed_growth_periods: Vec<u32> = <ProcessedGrowthPeriods<T>>::iter_keys().collect::<Vec<_>>();
    processed_growth_periods.sort();
    processed_growth_periods.reverse();
    let latest_processed_growth_period: u32 = processed_growth_periods.into_iter().nth(0).or_else(|| Some(0)).expect("we have a default value");
    
    <LastTriggeredGrowthPeriod<T>>::put(latest_processed_growth_period);
        
    Growth::<T>::translate::<GrowthInfo<T::AccountId, BalanceOf<T>>, _>(
        |period, mut growth_info| {
            add_weight(1, 1, Weight::from_ref_time(0));                        
            growth_info.added_by = None;
            growth_info.tx_id = None;
            growth_info.triggered = None;

            if period <= latest_processed_growth_period {                
                growth_info.tx_id = Some(0);
                growth_info.triggered = Some(true);
            }
            
            Some(growth_info)
        },
    );

    //Reads: [ProcessedGrowthPeriod], Writes: [LastTriggeredGrowthPeriod]
    add_weight(1, 1, Weight::from_ref_time(0));

    //Write: [STORAGE_VERSION]
    add_weight(0, 1, Weight::from_ref_time(0));
    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!("✅ Automatic growth migration completed successfully");

    // add a bit extra as safety margin for computation
    return consumed_weight + Weight::from_ref_time(25_000_000_000)
}

/// Migration to enable staking pallet
pub struct EnableAutomaticGrwoth<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for EnableAutomaticGrwoth<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain < 2 {
            log::info!(
                "💽 Running migration with current storage version {:?} / onchain {:?}",
                current,
                onchain
            );
            return enable_automatic_growth::<T>()
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
