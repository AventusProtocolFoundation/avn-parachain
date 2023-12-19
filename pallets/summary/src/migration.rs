// AvN is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
    Clone, Config, Decode, Encode, EthereumTransactionId, Pallet, RootData, Roots, TypeInfo, H256,
    STORAGE_VERSION,
};

use frame_support::{
    dispatch::GetStorageVersion,
    pallet_prelude::*,
    traits::{Get, OnRuntimeUpgrade},
    weights::Weight,
};
use sp_std::prelude::*;

#[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct OldRootData<AccountId> {
    pub root_hash: H256,
    pub added_by: Option<AccountId>,
    pub is_validated: bool,
    pub is_finalised: bool,
    pub tx_id: Option<u64>,
}

pub fn migrate_roots<T: Config>() -> Weight {
    let mut consumed_weight: Weight = Weight::from_ref_time(0);
    let mut add_weight = |reads, writes, weight: Weight| {
        consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
        consumed_weight += weight;
    };

    let mut counter = 0;

    Roots::<T>::translate::<OldRootData<T::AccountId>, _>(
        |_range, _ingress, old_root_data: OldRootData<T::AccountId>| {
            add_weight(1, 1, Weight::from_ref_time(0));

            let new_transaction_id: Option<EthereumTransactionId> =
                old_root_data.tx_id.map(|value| value.try_into().ok()).flatten();

            let new_root_data: RootData<T::AccountId> = RootData::new(
                old_root_data.root_hash,
                old_root_data.added_by.expect("Existing data will always have an added by"),
                new_transaction_id,
            );

            counter += 1;
            Some(new_root_data)
        },
    );

    //Write: [STORAGE_VERSION]
    add_weight(0, 1, Weight::from_ref_time(0));
    STORAGE_VERSION.put::<Pallet<T>>();

    log::info!(
        "✅ Summary root data migration completed successfully. {:?} entries migrated",
        counter
    );

    // add a bit extra as safety margin for computation
    return consumed_weight + Weight::from_ref_time(25_000_000_000)
}

/// Migration to enable staking pallet
pub struct MigrateSummaryRootData<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for MigrateSummaryRootData<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain < 1 {
            log::info!(
                "🚧 Running summary migration. Current storage version: {:?}, on-chain version: {:?}",
                current,
                onchain
            );
            return migrate_roots::<T>()
        }

        Weight::zero()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        Ok(vec![])
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_input: Vec<u8>) -> Result<(), &'static str> {
        let onchain_version = Pallet::<T>::on_chain_storage_version();
        frame_support::ensure!(onchain_version == 1, "must_upgrade");

        Ok(())
    }
}
