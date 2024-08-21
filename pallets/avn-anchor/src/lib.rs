#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use frame_support::{
    dispatch::DispatchResult, ensure, pallet_prelude::StorageVersion, traits::Get,
};
use frame_system::{self as system, ensure_none, ensure_root};
// use sp_std::prelude::*;

pub mod default_weights;
pub use default_weights::WeightInfo;

pub use pallet::*;
use sp_core::H256;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod benchmarking;

pub type MaximumHandlersBound = ConstU32<256>;

pub type ChainNameLimit = ConstU32<32>;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    pub type ChainId = u32;
    #[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(T))]
    pub struct ChainData {
        pub chain_id: ChainId,
        pub name: BoundedVec<u8, ChainNameLimit>,
    }
    pub type CheckpointId = u64;

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
        /// A new chain handler was registered. [handler_account_id, chain_id, name]
        ChainHandlerRegistered(T::AccountId, ChainId, BoundedVec<u8, ChainNameLimit>),
        /// A chain handler was updated. [old_handler_account_id, new_handler_account_id, chain_id,
        /// name]
        ChainHandlerUpdated(T::AccountId, T::AccountId, ChainId, BoundedVec<u8, ChainNameLimit>),
        //// A new checkpoint was submitted. [chain_id, checkpoint_id, checkpoint]
        CheckpointSubmitted(ChainId, CheckpointId, H256),
    }

    #[pallet::error]
    pub enum Error<T> {
        ChainNotRegistered,
        HandlerAlreadyRegistered,
        UnauthorizedHandler,
        NoAvailableChainId,
        EmptyChainName,
    }

    #[pallet::storage]
    #[pallet::getter(fn chain_handlers)]
    pub type ChainHandlers<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, ChainData>;

    #[pallet::storage]
    #[pallet::getter(fn next_chain_id)]
    pub type NextChainId<T> = StorageValue<_, ChainId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn checkpoints)]
    pub type Checkpoints<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        ChainId,
        Blake2_128Concat,
        CheckpointId,
        H256,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn next_checkpoint_id)]
    pub type NextCheckpointId<T: Config> =
        StorageMap<_, Blake2_128Concat, ChainId, CheckpointId, ValueQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(<T as pallet::Config>::WeightInfo::register_chain_handler())]
        #[pallet::call_index(0)]
        pub fn register_chain_handler(
            origin: OriginFor<T>,
            name: BoundedVec<u8, ChainNameLimit>,
        ) -> DispatchResult {
            let handler = ensure_signed(origin)?;

            ensure!(
                !ChainHandlers::<T>::contains_key(&handler),
                Error::<T>::HandlerAlreadyRegistered
            );

            ensure!(!name.is_empty(), Error::<T>::EmptyChainName);

            let chain_id = Self::get_next_chain_id()?;

            let chain_data = ChainData { chain_id, name: name.clone() };

            ChainHandlers::<T>::insert(&handler, chain_data);

            Self::deposit_event(Event::ChainHandlerRegistered(handler, chain_id, name));

            Ok(())
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::update_chain_handler())]
        #[pallet::call_index(1)]
        pub fn update_chain_handler(
            origin: OriginFor<T>,
            new_handler: T::AccountId,
        ) -> DispatchResult {
            let old_handler = ensure_signed(origin)?;
    
            ensure!(
                !ChainHandlers::<T>::contains_key(&new_handler),
                Error::<T>::HandlerAlreadyRegistered
            );
    
            ChainHandlers::<T>::try_mutate(&old_handler, |maybe_chain_data| -> DispatchResult {
                let chain_data = maybe_chain_data.take().ok_or(Error::<T>::ChainNotRegistered)?;
                ChainHandlers::<T>::insert(&new_handler, chain_data.clone());
    
                Self::deposit_event(Event::ChainHandlerUpdated(
                    old_handler.clone(),
                    new_handler.clone(),
                    chain_data.chain_id,
                    chain_data.name,
                ));
    
                Ok(())
            })?;
    
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::submit_checkpoint_with_identity())]
        #[pallet::call_index(2)]
        pub fn submit_checkpoint_with_identity(
            origin: OriginFor<T>,
            chain_id: ChainId,
            checkpoint: H256,
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;

            let authorized_handler =
                ChainHandlers::<T>::get(&chain_id).ok_or(Error::<T>::UnauthorizedHandler)?;
            ensure!(sender == authorized_handler, Error::<T>::UnauthorizedHandler);

            let checkpoint_id = Self::next_checkpoint_id(chain_id);

            Checkpoints::<T>::insert(chain_id, checkpoint_id, checkpoint);

            NextCheckpointId::<T>::mutate(chain_id, |id| *id += 1);

            Self::deposit_event(Event::CheckpointSubmitted(chain_id, checkpoint_id, checkpoint));

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn get_next_chain_id() -> Result<ChainId, DispatchError> {
            NextChainId::<T>::try_mutate(|id| {
                let current_id = *id;
                *id = id.checked_add(1).ok_or(Error::<T>::NoAvailableChainId)?;
                Ok(current_id)
            })
        }
    }
}
