// Copyright 2023 Aventus Network Services (UK) Ltd.

//! This pallet implements the AvN pallet's **BridgePublisher** interface, providing a **publish**
//! method which other pallets, implementing the **OnBridgePublisherResult**, can use to execute
//! any author function on the Ethereum-based **avn-bridge** contract. They do so
//! by passing the name of the desired avn-bridge function, along with an array of data type and
//! value parameter tuples. Upon receipt of a **publish** request, this pallet takes charge of
//! the entire transaction process. The process culminates in a callback to the originating pallet
//! detailing the final outcome, which can be used to commit or rollback state. Transaction requests
//! are handled sequentially and requests are queued if required.
//!
//! Specifically, the pallet manages:
//! - Accepting and managing external pallet requests to be processed to completion in the order
//!   they arrive.
//!
//! - The packaging and encoding of each transaction to ensure Ethereum compatibility.
//!
//! - The addition of a timestamp, delineating the deadline by which a transaction must reach the
//!   contract.
//!
//! - The addition of a unique transaction ID, against which request data can be stored on the AvN
//!   and the transaction's status in the avn-bridge be later checked.
//!
//! - Collection of the necessary ECDSA signatures from authors, labelled **confirmations**, which
//!   serve to prove AvN consensus for a transaction to the avn-bridge.
//!
//! - Appointing an author responsible for sending the transaction to Ethereum.
//!
//! - Utilising the transaction ID and expiry to check the status of a sent transaction on Ethereum
//!   and arrive at a consensus of that status by providing **corroborations**.
//!
//! - Alerting the originating pallet to the outcome via the OnBridgePublisherResult callback.
//!
//! The core of the pallet resides in the off-chain worker. The OCW monitors all unresolved
//! transactions, prompting authors to resolve them by invoking one of three unsigned extrinsics:
//!
//! 1. Before a transaction can be dispatched, confirmations are accumulated from non-sending
//!    authors via the **add_confirmation** extrinsic until a consensus is reached. Note: the
//!    sender's confirmation is taken as implicit by the avn-bridge and therefore not requested.
//!
//! 2. Once a transaction has received sufficient confirmations, the chosen sender is prompted to
//!    dispatch it to Ethereum and tag it as sent using the **add_eth_tx_hash** extrinsic.
//!
//! 3. When a transaction possesses an Ethereum tx hash, or if its expiration time has elapsed
//!    without a definitive outcome, all authors are requested to **add_corroboration**s. Achieiving
//!    consensus of corroborations determines the final state which is reported back to the
//!    originating pallet.

#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
};
use codec::{Decode, Encode, MaxEncodedLen};
use core::convert::TryInto;
use frame_support::{dispatch::DispatchResultWithPostInfo, log, traits::IsSubType, BoundedVec};
use frame_system::{
    ensure_none, ensure_root,
    offchain::{SendTransactionTypes, SubmitTransaction},
    pallet_prelude::OriginFor,
};
use pallet_avn::{self as avn, BridgePublisher, Error as avn_error, OnBridgePublisherResult};
use sp_application_crypto::RuntimeAppPublic;
use sp_avn_common::{event_types::Validator};
use sp_core::{ecdsa, ConstU32, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::{scale_info::TypeInfo, traits::Dispatchable};
use sp_std::prelude::*;

mod call;
mod eth;
mod tx;
mod util;

mod benchmarking;
#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;
#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

pub use pallet::*;
pub mod default_weights;
pub use default_weights::WeightInfo;

pub type AVN<T> = avn::Pallet<T>;
pub type Author<T> =
    Validator<<T as avn::Config>::AuthorityId, <T as frame_system::Config>::AccountId>;

pub type ConfirmationsLimit = ConstU32<100>; // Max confirmations or corroborations (must be > 1/3 of authors)
pub type FunctionLimit = ConstU32<32>; // Max chars allowed in T1 function name
pub type ParamsLimit = ConstU32<5>; // Max T1 function params (excluding expiry, t2TxId, and confirmations)
pub type TypeLimit = ConstU32<7>; // Max chars in a param's type
pub type ValueLimit = ConstU32<130>; // Max chars in a param's value

const PALLET_NAME: &'static [u8] = b"EthBridge";
const ADD_CONFIRMATION_CONTEXT: &'static [u8] = b"EthBridgeConfirmation";
const ADD_CORROBORATION_CONTEXT: &'static [u8] = b"EthBridgeCorroboration";
const ADD_ETH_TX_HASH_CONTEXT: &'static [u8] = b"EthBridgeEthTxHash";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::UnixTime, Blake2_128Concat};
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config:
        frame_system::Config + avn::Config + scale_info::TypeInfo + SendTransactionTypes<Call<Self>>
    {
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as frame_system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type TimeProvider: UnixTime;
        type WeightInfo: WeightInfo;
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>
            + IsSubType<Call<Self>>
            + From<Call<Self>>;
        type MaxQueuedTxRequests: Get<u32>;
        type AccountToBytesConvert: avn::AccountToBytesConverter<Self::AccountId>;
        type OnBridgePublisherResult: avn::OnBridgePublisherResult;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PublishToEthereum { tx_id: u32, function_name: Vec<u8>, params: Vec<(Vec<u8>, Vec<u8>)> },
        EthTxLifetimeUpdated { eth_tx_lifetime_secs: u64 },
        EthTxIdUpdated { eth_tx_id: u32 },
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn get_next_tx_id)]
    pub type NextTxId<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_eth_tx_lifetime_secs)]
    pub type EthTxLifetimeSecs<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type RequestQueue<T: Config> =
        StorageValue<_, BoundedVec<RequestData, T::MaxQueuedTxRequests>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_transaction_data)]
    pub type SettledTransactions<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, TransactionData<T>, OptionQuery>;

    #[pallet::storage]
    pub type ActiveTransaction<T: Config> = StorageValue<_, ActiveTransactionData<T>, OptionQuery>;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct RequestData {
        pub tx_id: u32,
        pub function_name: BoundedVec<u8, FunctionLimit>,
        pub params:
            BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct TransactionData<T: Config> {
        pub function_name: BoundedVec<u8, FunctionLimit>,
        pub params:
            BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
        pub sender: T::AccountId,
        pub eth_tx_hash: H256,
        pub tx_succeeded: bool,
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct ActiveTransactionData<T: Config> {
        pub id: u32,
        pub data: TransactionData<T>,
        pub expiry: u64,
        pub msg_hash: H256,
        pub confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
        pub success_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
        pub failure_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    }

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub _phantom: sp_std::marker::PhantomData<T>,
        pub eth_tx_lifetime_secs: u64,
        pub next_tx_id: u32,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { _phantom: Default::default(), eth_tx_lifetime_secs: 60 * 30, next_tx_id: 0 }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            EthTxLifetimeSecs::<T>::put(self.eth_tx_lifetime_secs);
            NextTxId::<T>::put(self.next_tx_id);
        }
    }

    #[pallet::error]
    pub enum Error<T> {
        CorroborateCallFailed,
        DuplicateConfirmation,
        EmptyFunctionName,
        ErrorAssigningSender,
        EthTxHashAlreadySet,
        EthTxHashMustBeSetBySender,
        ExceedsConfirmationLimit,
        ExceedsFunctionNameLimit,
        FunctionEncodingError,
        FunctionNameError,
        HandlePublishingResultFailed,
        InvalidBytes,
        InvalidBytesLength,
        InvalidCorroborateCalldata,
        InvalidCorroborateResult,
        InvalidECDSASignature,
        InvalidHashLength,
        InvalidHexString,
        InvalidParamData,
        InvalidSendCalldata,
        InvalidUint,
        InvalidUTF8,
        MsgHashError,
        ParamsLimitExceeded,
        ParamTypeEncodingError,
        SendTransactionFailed,
        TxRequestQueueFull,
        TypeNameLengthExceeded,
        ValueLengthExceeded,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::set_eth_tx_lifetime_secs())]
        pub fn set_eth_tx_lifetime_secs(
            origin: OriginFor<T>,
            eth_tx_lifetime_secs: u64,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;
            EthTxLifetimeSecs::<T>::put(eth_tx_lifetime_secs);
            Self::deposit_event(Event::<T>::EthTxLifetimeUpdated { eth_tx_lifetime_secs });
            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::set_eth_tx_id())]
        pub fn set_eth_tx_id(origin: OriginFor<T>, eth_tx_id: u32) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;
            NextTxId::<T>::put(eth_tx_id);
            Self::deposit_event(Event::<T>::EthTxIdUpdated { eth_tx_id });
            Ok(().into())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::add_confirmation())]
        pub fn add_confirmation(
            origin: OriginFor<T>,
            tx_id: u32,
            confirmation: ecdsa::Signature,
            author: Author<T>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            if tx::is_active::<T>(tx_id) {
                let mut tx = ActiveTransaction::<T>::get().expect("is active");
                log::info!("CONFIRMATIONS 1 !!! {:?}", tx.confirmations);

                // The sender's confirmation is implicit so we only collect them from other authors:
                if author.account_id == tx.data.sender || util::has_enough_confirmations(&tx) {
                    return Ok(().into())
                }

                eth::verify_signature::<T>(tx.msg_hash, &author, &confirmation)?;

                ensure!(
                    !tx.confirmations.contains(&confirmation),
                    Error::<T>::DuplicateConfirmation
                );
                log::info!("CONFIRMATIONS 1.5 !!! {:?}", confirmation);

                tx.confirmations
                    .try_push(confirmation)
                    .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;

                log::info!("CONFIRMATIONS 2 !!! {:?}", tx.confirmations);
                ActiveTransaction::<T>::put(tx);
            }

            Ok(().into())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::add_eth_tx_hash())]
        pub fn add_eth_tx_hash(
            origin: OriginFor<T>,
            tx_id: u32,
            eth_tx_hash: H256,
            author: Author<T>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            if tx::is_active::<T>(tx_id) {
                let mut tx = ActiveTransaction::<T>::get().expect("is active");

                ensure!(tx.data.eth_tx_hash == H256::zero(), Error::<T>::EthTxHashAlreadySet);

                ensure!(
                    tx.data.sender == author.account_id,
                    Error::<T>::EthTxHashMustBeSetBySender
                );

                tx.data.eth_tx_hash = eth_tx_hash;

                ActiveTransaction::<T>::put(tx);
            }

            Ok(().into())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::add_corroboration())]
        pub fn add_corroboration(
            origin: OriginFor<T>,
            tx_id: u32,
            tx_succeeded: bool,
            author: Author<T>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            if tx::is_active::<T>(tx_id) {
                let mut tx = ActiveTransaction::<T>::get().expect("is active");

                if !util::requires_corroboration(&tx, &author) {
                    return Ok(().into())
                }

                let matching_corroborations = if tx_succeeded {
                    &mut tx.success_corroborations
                } else {
                    &mut tx.failure_corroborations
                };

                matching_corroborations
                    .try_push(author.account_id.clone())
                    .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;

                if util::has_enough_corroborations::<T>(matching_corroborations.len()) {
                    tx::finalize_state::<T>(tx, tx_succeeded)?;
                } else {
                    ActiveTransaction::<T>::put(tx);
                }
            }

            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn offchain_worker(block_number: T::BlockNumber) {
            if let Ok(author) = setup_ocw::<T>(block_number) {
                if let Err(e) = process_active_transaction::<T>(author) {
                    log::error!("❌ Error processing currently active transaction: {:?}", e);
                }
            }
        }
    }

    fn setup_ocw<T: Config>(block_number: T::BlockNumber) -> Result<Author<T>, DispatchError> {
        AVN::<T>::pre_run_setup(block_number, PALLET_NAME.to_vec()).map_err(|e| {
            if e != DispatchError::from(avn_error::<T>::OffchainWorkerAlreadyRun) {
                log::error!("❌ Unable to run offchain worker: {:?}", e);
            }
            e
        })
    }

    // The core logic the OCW employs to fully resolve any currently active transaction:
    fn process_active_transaction<T: Config>(author: Author<T>) -> Result<(), DispatchError> {
        if let Some(tx) = ActiveTransaction::<T>::get() {
            let self_is_sender = author.account_id == tx.data.sender;
            let tx_has_enough_confirmations = util::has_enough_confirmations(&tx);
            let tx_is_sent = tx.data.eth_tx_hash != H256::zero();
            let tx_is_past_expiry = util::time_now::<T>() > tx.expiry;

            if !self_is_sender && !tx_has_enough_confirmations {
                let confirmation = eth::sign_msg_hash::<T>(&tx.msg_hash)?;
                if !tx.confirmations.contains(&confirmation) {
                    log::info!("ADDING CONFIRMATION !!! {:?}", confirmation);
                    call::add_confirmation::<T>(tx.id, confirmation, author);
                }
            } else if self_is_sender && tx_has_enough_confirmations && !tx_is_sent {
                let lock_name = format!("eth_bridge_ocw::send::{}", tx.id).as_bytes().to_vec();
                let mut lock = AVN::<T>::get_ocw_locker(&lock_name);

                // Protect against sending more than once
                if let Ok(guard) = lock.try_lock() {
                    let eth_tx_hash = eth::send_tx::<T>(&tx, &author)?;
                    call::add_eth_tx_hash::<T>(tx.id, eth_tx_hash, author);
                    guard.forget(); // keep the lock so we don't send again
                } else {
                    log::info!("👷 Skipping sending txId: {:?} because ocw is locked already.", tx.id);
                };
            } else if tx_is_sent || tx_is_past_expiry {
                if util::requires_corroboration::<T>(&tx, &author) {
                    let tx_status = eth::check_tx_status::<T>(tx.id, tx.expiry, &author)?;
                    log::info!("HELP process_active_transaction !!! {:?}", tx_status);
                    match tx_status {
                        Some(true) => call::add_corroboration::<T>(tx.id, true, author),
                        Some(false) => call::add_corroboration::<T>(tx.id, false, author),
                        None => {},
                    }
                }
            }
        }

        Ok(())
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;
        // Confirm that the call comes from an author before it can enter the pool:
        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                Call::add_confirmation { tx_id, confirmation, author, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(ADD_CONFIRMATION_CONTEXT, tx_id, confirmation, &author.account_id),
                        &author,
                        signature,
                    ) {
                        ValidTransaction::with_tag_prefix("EthBridgeAddConfirmation")
                            .and_provides((call, tx_id))
                            .priority(TransactionPriority::max_value())
                            .build()
                    } else {
                        InvalidTransaction::Custom(1u8).into()
                    },
                Call::add_eth_tx_hash { tx_id, eth_tx_hash, author, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(ADD_ETH_TX_HASH_CONTEXT, tx_id, eth_tx_hash, &author.account_id),
                        &author,
                        signature,
                    ) {
                        ValidTransaction::with_tag_prefix("EthBridgeAddReceipt")
                            .and_provides((call, tx_id))
                            .priority(TransactionPriority::max_value())
                            .build()
                    } else {
                        InvalidTransaction::Custom(2u8).into()
                    },
                Call::add_corroboration { tx_id, tx_succeeded, author, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, &author.account_id),
                        &author,
                        signature,
                    ) {
                        ValidTransaction::with_tag_prefix("EthBridgeAddCorroboration")
                            .and_provides((call, tx_id))
                            .priority(TransactionPriority::max_value())
                            .build()
                    } else {
                        InvalidTransaction::Custom(3u8).into()
                    },

                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    impl<T: Config> BridgePublisher for Pallet<T> {
        fn get_eth_tx_lifetime_secs() -> u64 {
            EthTxLifetimeSecs::<T>::get()
        }
        fn get_next_tx_id() -> u32 {
            NextTxId::<T>::get()
        }
        fn publish(
            function_name: &[u8],
            params: &[(Vec<u8>, Vec<u8>)],
        ) -> Result<u32, DispatchError> {
            let tx_id = tx::add_new_request::<T>(function_name, params)
                .map_err(|e| DispatchError::Other(e.into()))?;

            Self::deposit_event(Event::<T>::PublishToEthereum {
                tx_id,
                function_name: function_name.to_vec(),
                params: params.to_vec(),
            });

            Ok(tx_id)
        }
    }
}
