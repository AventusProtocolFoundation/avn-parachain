#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use frame_support::{dispatch::DispatchResult, ensure};

pub mod default_weights;
pub use default_weights::WeightInfo;

pub use pallet::*;
use sp_avn_common::CallDecoder;
use sp_core::{ConstU32, H256};
use sp_std::prelude::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod benchmarking;

pub type MaximumHandlersBound = ConstU32<256>;

pub type ChainNameLimit = ConstU32<32>;

pub const REGISTER_CHAIN_HANDLER: &'static [u8] = b"register_chain_handler";
pub const UPDATE_CHAIN_HANDLER: &'static [u8] = b"update_chain_handler";
pub const SUBMIT_CHECKPOINT: &'static [u8] = b"submit_checkpoint";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{dispatch::GetDispatchInfo, pallet_prelude::*, traits::IsSubType};
    use frame_system::pallet_prelude::*;
    use sp_avn_common::{verify_signature, InnerCallValidator, Proof};
    use sp_runtime::traits::{Dispatchable, IdentifyAccount, Verify};

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

        /// The overarching call type.
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>
            + IsSubType<Call<Self>>
            + From<Call<Self>>
            + GetDispatchInfo
            + From<frame_system::Call<Self>>;

        type Public: IdentifyAccount<AccountId = Self::AccountId>;

        /// The signature type used by accounts/transactions.
        type Signature: Verify<Signer = Self::Public>
            + Member
            + Decode
            + Encode
            + From<sp_core::sr25519::Signature>
            + TypeInfo;

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
        /// A new checkpoint was submitted. [handler_account_id, chain_id, checkpoint_id,
        /// checkpoint]
        CheckpointSubmitted(T::AccountId, ChainId, CheckpointId, H256),
    }

    #[pallet::error]
    pub enum Error<T> {
        ChainNotRegistered,
        HandlerAlreadyRegistered,
        UnauthorizedHandler,
        NoAvailableChainId,
        EmptyChainName,
        NoAvailableCheckpointId,
        UnauthorizedSignedTransaction,
        SenderNotValid,
        TransactionNotSupported,
        UnauthorizedProxyTransaction,
    }

    #[pallet::storage]
    #[pallet::getter(fn nonces)]
    pub type Nonces<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

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
    pub type NextCheckpointId<T> =
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

        #[pallet::weight(<T as pallet::Config>::WeightInfo::submit_checkpoint_with_identity())]
        #[pallet::call_index(2)]
        pub fn submit_checkpoint_with_identity(
            origin: OriginFor<T>,
            checkpoint: H256,
        ) -> DispatchResult {
            let handler = ensure_signed(origin)?;

            let chain_data =
                ChainHandlers::<T>::get(&handler).ok_or(Error::<T>::ChainNotRegistered)?;

            let checkpoint_id = Self::get_next_checkpoint_id(chain_data.chain_id)?;

            Checkpoints::<T>::insert(chain_data.chain_id, checkpoint_id, checkpoint);

            Self::deposit_event(Event::CheckpointSubmitted(
                handler,
                chain_data.chain_id,
                checkpoint_id,
                checkpoint,
            ));

            Ok(())
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::signed_register_chain_handler())]
        #[pallet::call_index(3)]
        pub fn signed_register_chain_handler(
            origin: OriginFor<T>,
            proof: Proof<T::Signature, T::AccountId>,
            handler: T::AccountId,
            name: BoundedVec<u8, ChainNameLimit>,
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(sender == handler, Error::<T>::SenderNotValid);
            let sender_nonce = Self::nonces(&sender);

            let signed_payload = Self::encode_signed_register_chain_handler_params(
                &proof.relayer,
                &handler,
                &name,
                sender_nonce,
            );

            ensure!(
                verify_signature::<T::Signature, T::AccountId>(&proof, &signed_payload).is_ok(),
                Error::<T>::UnauthorizedSignedTransaction
            );

            Self::do_register_chain_handler(&handler, name)?;

            <Nonces<T>>::mutate(&handler, |n| *n += 1);

            Ok(())
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::signed_update_chain_handler())]
        #[pallet::call_index(4)]
        pub fn signed_update_chain_handler(
            origin: OriginFor<T>,
            proof: Proof<T::Signature, T::AccountId>,
            old_handler: T::AccountId,
            new_handler: T::AccountId,
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(sender == old_handler, Error::<T>::SenderNotValid);
            let sender_nonce = Self::nonces(&sender);

            let signed_payload = Self::encode_signed_update_chain_handler_params(
                &proof,
                &old_handler,
                &new_handler,
                sender_nonce,
            );

            ensure!(
                verify_signature::<T::Signature, T::AccountId>(&proof, &signed_payload.as_slice())
                    .is_ok(),
                Error::<T>::UnauthorizedSignedTransaction
            );

            Self::do_update_chain_handler(&old_handler, &new_handler)?;

            <Nonces<T>>::mutate(&old_handler, |n| *n += 1);

            Ok(())
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::signed_submit_checkpoint_with_identity())]
        #[pallet::call_index(5)]
        pub fn signed_submit_checkpoint_with_identity(
            origin: OriginFor<T>,
            proof: Proof<T::Signature, T::AccountId>,
            handler: T::AccountId,
            checkpoint: H256,
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(sender == handler, Error::<T>::SenderNotValid);
            let sender_nonce = Self::nonces(&sender);

            let signed_payload = Self::encode_signed_submit_checkpoint_params(
                &proof,
                &handler,
                &checkpoint,
                sender_nonce,
            );

            ensure!(
                verify_signature::<T::Signature, T::AccountId>(&proof, &signed_payload.as_slice())
                    .is_ok(),
                Error::<T>::UnauthorizedSignedTransaction
            );

            Self::do_submit_checkpoint(&handler, checkpoint)?;

            <Nonces<T>>::mutate(&handler, |n| *n += 1);

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

        fn get_next_checkpoint_id(chain_id: ChainId) -> Result<CheckpointId, DispatchError> {
            NextCheckpointId::<T>::try_mutate(chain_id, |id| {
                let current_id = *id;
                *id = id.checked_add(1).ok_or(Error::<T>::NoAvailableCheckpointId)?;
                Ok(current_id)
            })
        }

        fn encode_signed_register_chain_handler_params(
            relayer: &T::AccountId,
            handler: &T::AccountId,
            name: &BoundedVec<u8, ChainNameLimit>,
            sender_nonce: u64,
        ) -> Vec<u8> {
            (REGISTER_CHAIN_HANDLER, relayer, handler, name, sender_nonce).encode()
        }

        fn encode_signed_update_chain_handler_params(
            proof: &Proof<T::Signature, T::AccountId>,
            old_handler: &T::AccountId,
            new_handler: &T::AccountId,
            sender_nonce: u64,
        ) -> Vec<u8> {
            (UPDATE_CHAIN_HANDLER, proof.relayer.clone(), old_handler, new_handler, sender_nonce)
                .encode()
        }

        fn encode_signed_submit_checkpoint_params(
            proof: &Proof<T::Signature, T::AccountId>,
            handler: &T::AccountId,
            checkpoint: &H256,
            sender_nonce: u64,
        ) -> Vec<u8> {
            (SUBMIT_CHECKPOINT, proof.relayer.clone(), handler, checkpoint, sender_nonce).encode()
        }

        fn do_register_chain_handler(
            handler: &T::AccountId,
            name: BoundedVec<u8, ChainNameLimit>,
        ) -> DispatchResult {
            ensure!(
                !ChainHandlers::<T>::contains_key(handler),
                Error::<T>::HandlerAlreadyRegistered
            );

            ensure!(!name.is_empty(), Error::<T>::EmptyChainName);

            let chain_id = Self::get_next_chain_id()?;

            let chain_data = ChainData { chain_id, name: name.clone() };

            ChainHandlers::<T>::insert(handler, chain_data);

            Self::deposit_event(Event::ChainHandlerRegistered(handler.clone(), chain_id, name));

            Ok(())
        }

        fn do_update_chain_handler(
            old_handler: &T::AccountId,
            new_handler: &T::AccountId,
        ) -> DispatchResult {
            ensure!(
                !ChainHandlers::<T>::contains_key(new_handler),
                Error::<T>::HandlerAlreadyRegistered
            );

            ChainHandlers::<T>::try_mutate(old_handler, |maybe_chain_data| -> DispatchResult {
                let chain_data = maybe_chain_data.take().ok_or(Error::<T>::ChainNotRegistered)?;
                ChainHandlers::<T>::insert(new_handler, chain_data.clone());

                Self::deposit_event(Event::ChainHandlerUpdated(
                    old_handler.clone(),
                    new_handler.clone(),
                    chain_data.chain_id,
                    chain_data.name,
                ));

                Ok(())
            })
        }

        fn do_submit_checkpoint(handler: &T::AccountId, checkpoint: H256) -> DispatchResult {
            let chain_data =
                ChainHandlers::<T>::get(handler).ok_or(Error::<T>::ChainNotRegistered)?;

            let checkpoint_id = Self::get_next_checkpoint_id(chain_data.chain_id)?;

            Checkpoints::<T>::insert(chain_data.chain_id, checkpoint_id, checkpoint);

            Self::deposit_event(Event::CheckpointSubmitted(
                handler.clone(),
                chain_data.chain_id,
                checkpoint_id,
                checkpoint,
            ));

            Ok(())
        }

        fn get_encoded_call_param(
            call: &<T as Config>::RuntimeCall,
        ) -> Option<(&Proof<T::Signature, T::AccountId>, Vec<u8>)> {
            let call = match call.is_sub_type() {
                Some(call) => call,
                None => return None,
            };

            match call {
                Call::signed_register_chain_handler { ref proof, ref handler, ref name } => {
                    let sender_nonce = Self::nonces(handler);
                    let encoded_data = Self::encode_signed_register_chain_handler_params(
                        &proof.relayer.clone(),
                        handler,
                        name,
                        sender_nonce,
                    );

                    Some((proof, encoded_data))
                },
                Call::signed_update_chain_handler {
                    ref proof,
                    ref old_handler,
                    ref new_handler,
                } => {
                    let sender_nonce = Self::nonces(old_handler);
                    let encoded_data = Self::encode_signed_update_chain_handler_params(
                        proof,
                        old_handler,
                        new_handler,
                        sender_nonce,
                    );

                    Some((proof, encoded_data))
                },
                Call::signed_submit_checkpoint_with_identity {
                    ref proof,
                    ref handler,
                    ref checkpoint,
                } => {
                    let sender_nonce = Self::nonces(handler);
                    let encoded_data = Self::encode_signed_submit_checkpoint_params(
                        proof,
                        handler,
                        checkpoint,
                        sender_nonce,
                    );

                    Some((proof, encoded_data))
                },
                _ => None,
            }
        }
    }

    impl<T: Config> CallDecoder for Pallet<T> {
        type AccountId = T::AccountId;
        type Signature = T::Signature;
        type Error = Error<T>;
        type Call = <T as Config>::RuntimeCall;

        fn get_proof(
            call: &Self::Call,
        ) -> Result<Proof<Self::Signature, Self::AccountId>, Self::Error> {
            let call = match call.is_sub_type() {
                Some(call) => call,
                None => return Err(Error::<T>::TransactionNotSupported),
            };

            match call {
                Call::signed_register_chain_handler { proof, .. } => Ok(proof.clone()),
                Call::signed_update_chain_handler { proof, .. } => Ok(proof.clone()),
                Call::signed_submit_checkpoint_with_identity { proof, .. } => Ok(proof.clone()),
                _ => Err(Error::<T>::TransactionNotSupported),
            }
        }
    }

    impl<T: Config> InnerCallValidator for Pallet<T> {
        type Call = <T as Config>::RuntimeCall;

        fn signature_is_valid(call: &Box<Self::Call>) -> bool {
            if let Some((proof, signed_payload)) = Self::get_encoded_call_param(call) {
                return verify_signature::<T::Signature, T::AccountId>(
                    &proof,
                    &signed_payload.as_slice(),
                )
                .is_ok();
            }

            return false;
        }
    }
}