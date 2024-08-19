#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

// use pallet_avn::{self as avn};
use frame_support::{
    dispatch::DispatchResult, ensure, pallet_prelude::StorageVersion, traits::Get,
};
use frame_system::{self as system, ensure_none, ensure_root};
// use sp_std::prelude::*;

pub mod default_weights;
pub use default_weights::WeightInfo;

pub use pallet::*;
use sp_core::ConstU32;

pub type MaximumHandlersBound = ConstU32<256>;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    pub type ChainId = u32;

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_avn::Config {
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as frame_system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new chain handler was registered. [chain_id, handler_account_id]
        ChainHandlerRegistered(ChainId, T::AccountId),
        /// A chain handler was updated. [chain_id, new_handler_account_id]
        ChainHandlerUpdated(ChainId, T::AccountId),
    }

    #[pallet::error]
    pub enum Error<T> {
        HandlerAlreadyExists,
        HandlerNotRegistered,
    }

    #[pallet::storage]
    #[pallet::getter(fn chain_handlers)]
    pub type ChainHandlers<T: Config> = StorageMap<_, Blake2_128Concat, ChainId, T::AccountId>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(<T as pallet::Config>::WeightInfo::register_chain_handler())]
        #[pallet::call_index(0)]
        pub fn register_chain_handler(
            origin: OriginFor<T>,
            chain_id: ChainId,
            handler_account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?; // Assuming only the root can register handlers

            ensure!(!ChainHandlers::<T>::contains_key(&chain_id), Error::<T>::HandlerAlreadyExists);

            ChainHandlers::<T>::insert(chain_id, handler_account_id.clone());

            Self::deposit_event(Event::ChainHandlerRegistered(chain_id, handler_account_id.clone()));

            Ok(())
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::update_chain_handler())]
        #[pallet::call_index(1)]
        pub fn update_chain_handler(
            origin: OriginFor<T>,
            chain_id: ChainId,
            new_handler_account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            ChainHandlers::<T>::try_mutate(&chain_id, |maybe_handler| -> DispatchResult {
                let handler = maybe_handler.as_mut().ok_or(Error::<T>::HandlerNotRegistered)?;
                *handler = new_handler_account_id.clone();
                Ok(())
            })?;

            Self::deposit_event(Event::ChainHandlerUpdated(chain_id, new_handler_account_id.clone()));

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
