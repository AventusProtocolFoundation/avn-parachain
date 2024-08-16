pub use pallet::*;

use pallet_avn::{self as avn};
use frame_system::{
    self as system, ensure_none, ensure_root,
    offchain::{SendTransactionTypes, SubmitTransaction},
};
use frame_support::{
    dispatch::DispatchResult, ensure, pallet_prelude::StorageVersion, traits::Get,
};

pub mod default_weights;
pub use default_weights::WeightInfo;

const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config:
        SendTransactionTypes<Call<Self>> + frame_system::Config + avn::Config
    {
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as frame_system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {}

    #[pallet::error]
    pub enum Error<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(<T as pallet::Config>::WeightInfo::register_chain_handler())]
        #[pallet::call_index(0)]
        pub fn register_chain_handler(
            origin: OriginFor<T>,
            // handler:
        ) -> DispatchResult {
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::update_chain_handler())]
        #[pallet::call_index(1)]
        pub fn update_chain_handler(
            origin: OriginFor<T>,
            // handler:
        ) -> DispatchResult {
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::submit_checkpoint_with_identity())]
        #[pallet::call_index(2)]
        pub fn submit_checkpoint_with_identity(
            origin: OriginFor<T>,
            // checkpoint
            // identity
        ) -> DispatchResult {
            Ok(())
        }
    }
}
