// Copyright 2023 Aventus Network Services (UK) Ltd.

//! This pallet implements the AvN pallet's **BridgePublisher** interface, providing a **publish**
//! method which other pallets, implementing the **OnPublishingResultHandler**, can use to execute
//! any author functions on the Ethereum-based **avn-bridge** contract. They do so
//! by passing the name of the desired avn-bridge function, along with an array of data type and
//! value parameter tuples. Upon receipt of a **publish** request, this pallet takes charge of the
//! entire transaction process. The process culminates in a callback to the originating pallet detailing the
//! final outcome, which may be used to commit or rollback state.
//!
//! Specifically, the pallet manages:
//!
//! - The packaging and encoding of the transaction to ensure Ethereum compatibility.
//!
//! - The addition of a timestamp, delineating the deadline by which the transaction must reach the
//!   contract.
//!
//! - The addition of a unique transaction ID, against which request data can be stored on the AvN
//!   and the transaction's status in the avn-bridge be later checked.
//!
//! - Collection of the necessary ECDSA signatures from authors, labelled **confirmations**, which
//!   serve to prove AvN consensus for the transaction to the avn-bridge.
//!
//! - Appointing an author responsible for sending the transaction to Ethereum.
//!
//! - Utilising the transaction ID and expiry to check the status of a sent transaction on Ethereum
//!   and arrive at a consensus of that status by providing **corroborations**.
//!
//! - Alerting the originating pallet to the outcome via the OnPublishingResultHandler callback.
//!
//! The core of the pallet resides in the off-chain worker. The OCW monitors all unresolved
//! transactions, prompting authors to resolve them by invoking one of three unsigned extrinsics:
//!
//! 1. Before a transaction can be dispatched, confirmations are accumulated from non-sending
//!    authors via the **add_confirmation** extrinsic until a consensus is reached. Note: the
//!    sender's confirmation is taken as implicit by the avn-bridge and is therefore not requested.
//!
//! 2. Once a transaction has received sufficient confirmations, the chosen sender is prompted to
//!    dispatch it to Ethereum and tag it as sent using the **add_receipt** extrinsic.
//!
//! 3. Finally, when a transaction possesses a receipt, or if its expiration time has elapsed
//!    without a definitive outcome, all authors are requested to **add_corroboration**s which, upon
//!    reaching consensus, conclusively determine the final state to report.

#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use codec::{Decode, Encode, MaxEncodedLen};
use core::convert::TryInto;
use frame_support::{dispatch::DispatchResultWithPostInfo, log, traits::IsSubType, BoundedVec};
use frame_system::{
    ensure_none, ensure_root,
    offchain::{SendTransactionTypes, SubmitTransaction},
    pallet_prelude::OriginFor,
};
use pallet_avn::{self as avn, BridgePublisher, Error as avn_error, OnPublishingResultHandler};
use sp_application_crypto::RuntimeAppPublic;
use sp_avn_common::event_types::Validator;
use sp_core::{ecdsa, ConstU32, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::{scale_info::TypeInfo, traits::Dispatchable};

mod call;
mod eth;
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
const ADD_RECEIPT_CONTEXT: &'static [u8] = b"EthBridgeReceipt";

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
        type MaxUnresolvedTx: Get<u32>;
        type AccountToBytesConvert: avn::AccountToBytesConverter<Self::AccountId>;
        type OnPublishingResultHandler: avn::OnPublishingResultHandler;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PublishToEthereum { tx_id: u32, function_name: Vec<u8>, params: Vec<(Vec<u8>, Vec<u8>)> },
        EthTxLifetimeUpdated { eth_tx_lifetime_secs: u64 },
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn get_next_tx_id)]
    pub type NextTxId<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_eth_tx_lifetime_secs)]
    pub type EthTxLifetimeSecs<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type UnresolvedTxs<T: Config> =
        StorageValue<_, BoundedVec<u32, T::MaxUnresolvedTx>, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub _phantom: sp_std::marker::PhantomData<T>,
        pub eth_tx_lifetime_secs: u64,
        pub next_tx_id: u32,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { _phantom: Default::default(), eth_tx_lifetime_secs: 0, next_tx_id: 0 }
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
        CalldataGenerationFailed,
        ContractCallFailed,
        CorroborateCallFailed,
        CorroborationNotFound,
        DeadlineReached,
        DuplicateConfirmation,
        ErrorAssigningSender,
        EthTxHashAlreadySet,
        EthTxHashMustBeSetBySender,
        ExceedsConfirmationLimit,
        ExceedsFunctionNameLimit,
        FunctionEncodingError,
        FunctionNameError,
        HandlePublishingResultFailed,
        InvalidBytes,
        InvalidCalldataGeneration,
        InvalidEthereumCheckResponse,
        InvalidData,
        InvalidDataLength,
        InvalidECDSASignature,
        InvalidHashLength,
        InvalidHexString,
        InvalidUint,
        InvalidUTF8Bytes,
        MsgHashError,
        ParamsLimitExceeded,
        ParamTypeEncodingError,
        ParamValueEncodingError,
        RequestTimedOut,
        TxIdNotFound,
        TypeNameLengthExceeded,
        UnexpectedStatusCode,
        UnresolvedTxLimitReached,
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
        #[pallet::weight(<T as Config>::WeightInfo::add_confirmation())]
        pub fn add_confirmation(
            origin: OriginFor<T>,
            tx_id: u32,
            confirmation: ecdsa::Signature,
            author: Author<T>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            util::update_confirmations::<T>(tx_id, &confirmation, &author)?;
            Ok(().into())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::add_receipt())]
        pub fn add_receipt(
            origin: OriginFor<T>,
            tx_id: u32,
            eth_tx_hash: H256,
            author: Author<T>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            util::set_eth_tx_hash::<T>(tx_id, eth_tx_hash, &author)?;
            Ok(().into())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::add_corroboration())]
        pub fn add_corroboration(
            origin: OriginFor<T>,
            tx_id: u32,
            tx_succeeded: bool,
            author: Author<T>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            util::update_corroborations::<T>(tx_id, tx_succeeded, &author)?;
            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn offchain_worker(block_number: T::BlockNumber) {
            if let Ok(author) = setup_ocw::<T>(block_number) {
                for tx_id in UnresolvedTxs::<T>::get() {
                    if let Err(e) = process_unresolved_tx::<T>(tx_id, author.clone()) {
                        log::error!("Error processing tx_id {}: {:?}", tx_id, e);
                    }
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

    // The core logic being triggered by the OCW until a tx gets resolved:
    fn process_unresolved_tx<T: Config>(
        tx_id: u32,
        author: Author<T>,
    ) -> Result<(), DispatchError> {
        let tx_data = Transactions::<T>::get(tx_id).ok_or(Error::<T>::TxIdNotFound)?;
        let self_is_sender = author.account_id == tx_data.sender;
        let tx_is_sent = tx_data.eth_tx_hash != H256::zero();
        let tx_is_past_expiry = tx_data.expiry > util::time_now::<T>();
        let num_confirmations = tx_data.confirmations.len() as u32 + 1; // include sender
        let tx_requires_confirmations = util::quorum_reached::<T>(num_confirmations) == false;

        if !self_is_sender && tx_requires_confirmations {
            let confirmation = eth::sign_msg_hash::<T>(tx_data.msg_hash)?;
            if !tx_data.confirmations.contains(&confirmation) {
                call::add_confirmation::<T>(tx_id, confirmation, author);
            }
        } else if self_is_sender && !tx_is_sent {
            let eth_tx_hash = eth::send_transaction::<T>(tx_id, author.clone())?;
            call::add_receipt::<T>(tx_id, eth_tx_hash, author);
        } else if tx_is_sent || tx_is_past_expiry {
            if util::requires_corroboration::<T>(tx_id, &author)? {
                match eth::check_tx_status::<T>(tx_id, tx_data.expiry, &author) {
                    Ok(EthStatus::Unresolved) => {},
                    Ok(EthStatus::Succeeded) => {
                        call::add_corroboration::<T>(tx_id, true, author);
                    },
                    Ok(EthStatus::Failed) => {
                        call::add_corroboration::<T>(tx_id, false, author);
                    },
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(())
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                Call::add_confirmation { tx_id, confirmation, author, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(ADD_CONFIRMATION_CONTEXT, tx_id, confirmation, author),
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
                Call::add_receipt { tx_id, eth_tx_hash, author, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(ADD_RECEIPT_CONTEXT, tx_id, eth_tx_hash, author),
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
                        &(ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, author),
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

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub enum EthStatus {
        #[default]
        Unresolved,
        Succeeded,
        Failed,
    }

    #[pallet::storage]
    #[pallet::getter(fn get_transaction_data)]
    pub type Transactions<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, TransactionData<T>, OptionQuery>;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct TransactionData<T: Config> {
        pub function_name: BoundedVec<u8, FunctionLimit>,
        pub params:
            BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
        pub expiry: u64,
        pub msg_hash: H256,
        pub confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
        pub sender: T::AccountId,
        pub eth_tx_hash: H256,
        pub status: EthStatus,
    }

    #[pallet::storage]
    #[pallet::getter(fn get_corroboration_data)]
    pub type Corroborations<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CorroborationData<T>, OptionQuery>;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct CorroborationData<T: Config> {
        pub tx_succeeded: BoundedVec<T::AccountId, ConfirmationsLimit>,
        pub tx_failed: BoundedVec<T::AccountId, ConfirmationsLimit>,
    }

    impl<T: Config> BridgePublisher for Pallet<T> {
        fn publish(
            function_name: &[u8],
            params: &[(Vec<u8>, Vec<u8>)],
        ) -> Result<u32, DispatchError> {
            let tx_id = util::use_next_tx_id::<T>();
            let expiry = util::time_now::<T>() + Self::get_eth_tx_lifetime_secs();

            let tx_data = eth::create_tx_data(function_name, params, expiry, tx_id)
                .map_err(|_| DispatchError::Other("Failed to create tx data"))?;

            util::add_new_tx_request::<T>(tx_id, tx_data)
                .map_err(|_| DispatchError::Other("Failed to add new tx request"))?;

            Self::deposit_event(Event::<T>::PublishToEthereum {
                tx_id,
                function_name: function_name.to_vec(),
                params: params.to_vec(),
            });

            Ok(tx_id)
        }
    }
}
