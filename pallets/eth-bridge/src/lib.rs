// Copyright 2023 Aventus Network Services (UK) Ltd.

//! This pallet provides a single interface, **publish_to_avn_bridge**, which enables other pallets
//! to execute any author-accessible function on the Ethereum-based **avn-bridge** contract. Pallets
//! do so by passing the desired avn-bridge function name, along with an array of parameter tuples,
//! each comprising the data type and its corresponding value. Upon receipt of a
//! **publish_to_avn_bridge** request this pallet takes charge of the entire transaction process,
//! culminating in the conclusive determination of the transaction's status on Ethereum. A callback
//! is then made to the originating pallet which it can use to determine whether to commit or
//! rollback state.
//!
//! Specifically, the pallet manages:
//!
//! - The packaging and encoding of the transaction to ensure Ethereum compatibility.
//!
//! - The addition of a timestamp, delineating the deadline by which the transaction must reach the
//!   contract.
//!
//! - The addition of a unique transaction ID, against which request data can be stored on the AvN
//!   and the transaction's status in the avn-bridge can be later checked.
//!
//! - Collection of the necessary ECDSA signatures from authors, labelled **confirmations**, which
//!   serve to prove AvN consensus for the transaction to the avn-bridge.
//!
//! - Appointing an author responsible for sending the transaction to Ethereum.
//!
//! - Utilising the transaction ID and expiry to check the status of a sent transaction on Ethereum
//!   and arrive at a consensus of that status by providing **corroborations**.
//!
//! - Alerting the originating pallet to the outcome via the HandleAvnBridgeResult callback.
//!
//! The core of the pallet resides in the off-chain worker. The OCW monitors all unresolved
//! transactions, prompting authors to resolve them by invoking one of three unsigned extrinsics:
//!
//! 1. Before a transaction can be dispatched, confirmations are accumulated from non-sending
//!    authors via the **add_confirmation** extrinsic until a consensus is reached. Note: the
//!    sender's confirmation is implicit.
//!
//! 2. Once a transaction has received sufficient confirmations, the chosen sender is prompted to
//!    dispatch it to Ethereum and tag it as sent using the **add_receipt** extrinsic.
//!
//! 3. Finally, when a transaction possesses a receipt, or if its expiration time has elapsed
//!    without a definitive outcome, all authors except the sender are requested to
//!    **add_corroboration**s which will determine the final state.

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
use ethabi::{Function, Int, Param, ParamType, Token};
use frame_support::{dispatch::DispatchResultWithPostInfo, log, traits::IsSubType, BoundedVec};
use frame_system::{
    ensure_none, ensure_root,
    offchain::{SendTransactionTypes, SubmitTransaction},
    pallet_prelude::OriginFor,
};
use hex_literal::hex;
use pallet_avn::{self as avn, AccountToBytesConverter, Error as avn_error};
use sp_application_crypto::RuntimeAppPublic;
use sp_avn_common::{calculate_one_third_quorum, event_types::Validator, EthTransaction};
use sp_core::{ecdsa, H160, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::{
    offchain::{http, Duration},
    scale_info::TypeInfo,
    traits::Dispatchable,
};

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

pub const CONFIRMATIONS_LIMIT: u32 = 100; // Max confirmations or corroborations (must be > 1/3 of authors)
pub const FUNCTION_NAME_CHAR_LIMIT: u32 = 32; // Max chars in T1 function name
pub const PARAMS_LIMIT: u32 = 5; // Max T1 function params (excluding expiry, t2TxId, and confirmations)
pub const TYPE_CHAR_LIMIT: u32 = 7; // Max chars in a param's type
pub const VALUE_CHAR_LIMIT: u32 = 130; // Max chars in a param's value

const PALLET_NAME: &'static [u8] = b"EthBridge";
const ADD_CONFIRMATION_CONTEXT: &'static [u8] = b"EthBridgeConfirmation";
const ADD_CORROBORATION_CONTEXT: &'static [u8] = b"EthBridgeCorroboration";
const ADD_RECEIPT_CONTEXT: &'static [u8] = b"EthBridgeReceipt";

const UINT256: &[u8] = b"uint256";
const UINT128: &[u8] = b"uint128";
const UINT32: &[u8] = b"uint32";
const BYTES: &[u8] = b"bytes";
const BYTES32: &[u8] = b"bytes32";

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
        type HandleAvnBridgeResult: HandleAvnBridgeResult;
    }

    pub trait HandleAvnBridgeResult {
        type Error: Into<sp_runtime::DispatchError>;
        fn result(tx_id: u32, tx_succeeded: bool) -> Result<(), Self::Error>;
    }

    pub type Author<T> =
        Validator<<T as avn::Config>::AuthorityId, <T as frame_system::Config>::AccountId>;

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
    pub type UnresolvedTxList<T: Config> =
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
        DeadlineReached,
        DuplicateConfirmation,
        ErrorAssigningSender,
        EthTxHashAlreadySet,
        EthTxHashMustBeSetBySender,
        ExceedsConfirmationLimit,
        ExceedsFunctionNameLimit,
        FunctionEncodingError,
        FunctionNameError,
        HandleAvnBridgeResultFailed,
        InvalidBytes,
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
            // SUDO
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

            let tx_data = Transactions::<T>::get(tx_id).ok_or(Error::<T>::TxIdNotFound)?;
            if !AVN::<T>::eth_signature_is_valid(
                (&tx_data.msg_hash).to_string(),
                &author,
                &confirmation,
            ) {
                return Err(avn_error::<T>::InvalidECDSASignature)?
            };

            Transactions::<T>::try_mutate_exists(
                tx_id,
                |maybe_tx_data| -> DispatchResultWithPostInfo {
                    let tx_data = maybe_tx_data.as_mut().ok_or(Error::<T>::TxIdNotFound)?;

                    if tx_data.confirmations.contains(&confirmation) {
                        return Err(Error::<T>::DuplicateConfirmation.into())
                    }

                    tx_data
                        .confirmations
                        .try_push(confirmation)
                        .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;

                    Ok(().into())
                },
            )
        }

        #[pallet::call_index(2)]
        #[pallet::weight(10_000)] // TODO: set weight
        pub fn add_receipt(
            origin: OriginFor<T>,
            tx_id: u32,
            eth_tx_hash: H256,
            author: Author<T>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            let mut tx_data = Transactions::<T>::get(tx_id).ok_or(Error::<T>::TxIdNotFound)?;

            ensure!(tx_data.eth_tx_hash == H256::zero(), Error::<T>::EthTxHashAlreadySet);
            ensure!(tx_data.sender == author.account_id, Error::<T>::EthTxHashMustBeSetBySender);

            tx_data.eth_tx_hash = eth_tx_hash;
            <Transactions<T>>::insert(tx_id, tx_data);

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

            if !UnresolvedTxList::<T>::get().contains(&tx_id) {
                return Ok(().into())
            }

            let mut corroborations = Corroborations::<T>::get(tx_id).unwrap();
            let num_corroborations;

            if tx_succeeded {
                if !corroborations.success.contains(&author.account_id) {
                    let mut tmp_vec = Vec::new();
                    tmp_vec.push(author.account_id);
                    corroborations
                        .success
                        .try_append(&mut tmp_vec)
                        .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;
                }
                num_corroborations = corroborations.success.len() as u32;
            } else {
                if !corroborations.failure.contains(&author.account_id) {
                    let mut tmp_vec = Vec::new();
                    tmp_vec.push(author.account_id);
                    corroborations
                        .failure
                        .try_append(&mut tmp_vec)
                        .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;
                }
                num_corroborations = corroborations.failure.len() as u32;
            }

            if Self::consensus_is_reached(num_corroborations) {
                Self::finalize_state(tx_id, tx_succeeded)?;
            } else {
                <Corroborations<T>>::insert(tx_id, corroborations);
            }

            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn offchain_worker(block_number: T::BlockNumber) {
            if let Ok(author) = setup_ocw::<T>(block_number) {
                for tx_id in UnresolvedTxList::<T>::get() {
                    Self::process_unresolved_transaction(tx_id, author.clone());
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
    pub enum EthTxStatus {
        #[default]
        Unresolved,
        Succeeded,
        Failed,
    }

    pub type FunctionLimit = ConstU32<FUNCTION_NAME_CHAR_LIMIT>;
    pub type ParamsLimit = ConstU32<PARAMS_LIMIT>;
    pub type TypeLimit = ConstU32<TYPE_CHAR_LIMIT>;
    pub type ValueLimit = ConstU32<VALUE_CHAR_LIMIT>;
    pub type ConfirmationsLimit = ConstU32<CONFIRMATIONS_LIMIT>;

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
        pub status: EthTxStatus,
    }

    #[pallet::storage]
    #[pallet::getter(fn get_corroboration_data)]
    pub type Corroborations<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CorroborationData<T>, OptionQuery>;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct CorroborationData<T: Config> {
        pub success: BoundedVec<T::AccountId, ConfirmationsLimit>,
        pub failure: BoundedVec<T::AccountId, ConfirmationsLimit>,
    }

    impl<T: Config> Pallet<T> {
        // The sole entry point for other pallets:
        pub fn publish_to_avn_bridge(
            function_name: &[u8],
            params: &[(Vec<u8>, Vec<u8>)],
        ) -> Result<u32, Error<T>> {
            // Quick sanity check:
            _ = String::from_utf8(function_name.to_vec())
                .map_err(|_| Error::<T>::FunctionNameError)?;

            let tx_id = Self::use_next_tx_id();
            let expiry = Self::time_now() + Self::get_eth_tx_lifetime_secs();

            let mut extended_params = params.to_vec();
            extended_params.push((UINT256.to_vec(), expiry.to_string().into_bytes()));
            extended_params.push((UINT32.to_vec(), tx_id.to_string().into_bytes()));

            let tx_data = TransactionData {
                function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
                    .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
                params: Self::bound_params(params.to_vec())?,
                expiry,
                msg_hash: Self::generate_msg_hash(&extended_params)?,
                confirmations: BoundedVec::<ecdsa::Signature, ConfirmationsLimit>::default(),
                sender: Self::assign_sender()?,
                eth_tx_hash: H256::zero(),
                status: EthTxStatus::Unresolved,
            };

            let corroborations = CorroborationData {
                success: BoundedVec::default(),
                failure: BoundedVec::default(),
            };

            <Transactions<T>>::insert(tx_id, tx_data);
            <Corroborations<T>>::insert(tx_id, corroborations);
            Self::add_to_unresolved_tx_list(tx_id)?;

            Self::deposit_event(Event::<T>::PublishToEthereum {
                tx_id,
                function_name: function_name.to_vec(),
                params: params.to_vec(),
            });

            Ok(tx_id)
        }

        // The core logic being triggered by the OCW hook:
        fn process_unresolved_transaction(tx_id: u32, author: Author<T>) {
            let tx_data = match Transactions::<T>::get(tx_id) {
                Some(data) => data,
                None => {
                    log::error!("Transaction not found for tx_id: {}", tx_id);
                    return
                },
            };

            let this_author_is_sender = author.account_id == tx_data.sender;
            let num_confirmations = 1 + tx_data.confirmations.len() as u32; // The sender's confirmation is implicit

            if !Self::consensus_is_reached(num_confirmations) {
                if !this_author_is_sender {
                    Self::provide_confirmation(tx_id, tx_data, author);
                }
            } else if this_author_is_sender {
                Self::send_transaction_to_ethereum(tx_id, author);
            } else {
                Self::attempt_to_confirm_eth_tx_status(tx_id, tx_data, author);
            }
        }

        fn assign_sender() -> Result<T::AccountId, Error<T>> {
            let current_block_number = <frame_system::Pallet<T>>::block_number();

            match AVN::<T>::calculate_primary_validator(current_block_number) {
                Ok(primary_validator) => {
                    let sender = primary_validator;
                    Ok(sender)
                },
                Err(_) => Err(Error::<T>::ErrorAssigningSender),
            }
        }

        fn use_next_tx_id() -> u32 {
            let tx_id = NextTxId::<T>::get();
            NextTxId::<T>::put(tx_id + 1);
            tx_id
        }

        fn consensus_is_reached(entries: u32) -> bool {
            let quorum = calculate_one_third_quorum(AVN::<T>::validators().len() as u32);
            entries >= quorum
        }

        fn time_now() -> u64 {
            <T as pallet::Config>::TimeProvider::now().as_secs()
        }

        fn add_to_unresolved_tx_list(tx_id: u32) -> Result<(), Error<T>> {
            UnresolvedTxList::<T>::try_mutate(|txs| -> Result<(), Error<T>> {
                txs.try_push(tx_id).map_err(|_| Error::<T>::UnresolvedTxLimitReached)
            })
        }

        fn remove_from_unresolved_tx_list(tx_id: u32) -> DispatchResult {
            UnresolvedTxList::<T>::mutate(|txs| {
                if let Some(pos) = txs.iter().position(|&x| x == tx_id) {
                    txs.remove(pos);
                }
            });
            Ok(())
        }

        fn bound_params(
            params: Vec<(Vec<u8>, Vec<u8>)>,
        ) -> Result<
            BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
            Error<T>,
        > {
            let result: Result<Vec<_>, _> = params
                .into_iter()
                .map(|(type_vec, value_vec)| {
                    let type_bounded = BoundedVec::try_from(type_vec)
                        .map_err(|_| Error::<T>::TypeNameLengthExceeded)?;
                    let value_bounded = BoundedVec::try_from(value_vec)
                        .map_err(|_| Error::<T>::ValueLengthExceeded)?;
                    Ok::<_, Error<T>>((type_bounded, value_bounded))
                })
                .collect();

            BoundedVec::<_, ParamsLimit>::try_from(result?)
                .map_err(|_| Error::<T>::ParamsLimitExceeded)
        }

        fn unbound_params(
            params: BoundedVec<
                (BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>),
                ParamsLimit,
            >,
        ) -> Vec<(Vec<u8>, Vec<u8>)> {
            params
                .into_iter()
                .map(|(type_bounded, value_bounded)| (type_bounded.into(), value_bounded.into()))
                .collect()
        }

        fn generate_msg_hash(params: &[(Vec<u8>, Vec<u8>)]) -> Result<H256, Error<T>> {
            let tokens: Result<Vec<_>, _> = params
                .iter()
                .map(|(type_bytes, value_bytes)| {
                    let param_type =
                        Self::to_param_type(type_bytes).ok_or_else(|| Error::<T>::MsgHashError)?;
                    Self::to_token_type(&param_type, value_bytes)
                })
                .collect();

            let encoded = ethabi::encode(&tokens?);
            let msg_hash = keccak_256(&encoded);

            Ok(H256::from(msg_hash))
        }

        fn provide_confirmation(tx_id: u32, tx_data: TransactionData<T>, author: Author<T>) {
            match Self::sign_msg_hash_to_create_confirmation(tx_data.msg_hash) {
                Ok(confirmation) => {
                    let proof = Self::encode_add_confirmation_proof(
                        tx_id,
                        confirmation.clone(),
                        author.clone(),
                    );

                    if let Some(signature) = author.key.sign(&proof) {
                        let call =
                            Call::<T>::add_confirmation { tx_id, confirmation, author, signature };
                        if SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
                            .is_err()
                        {
                            log::error!(
                                "❌ Error submitting unsigned transaction for confirmation."
                            );
                        }
                    } else {
                        log::error!("❌ Error signing proof.");
                    }
                },
                Err(err) => {
                    log::error!("❌ Error signing confirmation: {:?}", err);
                },
            }
        }

        fn sign_msg_hash_to_create_confirmation(
            msg_hash: H256,
        ) -> Result<ecdsa::Signature, DispatchError> {
            let msg_hash_string = msg_hash.to_string();
            let confirmation =
                AVN::<T>::request_ecdsa_signature_from_external_service(&msg_hash_string)?;
            Ok(confirmation)
        }

        fn encode_add_confirmation_proof(
            tx_id: u32,
            confirmation: ecdsa::Signature,
            author: Author<T>,
        ) -> Vec<u8> {
            (ADD_CONFIRMATION_CONTEXT, tx_id, confirmation, author.account_id).encode()
        }

        fn attempt_to_confirm_eth_tx_status(
            tx_id: u32,
            tx_data: TransactionData<T>,
            author: Author<T>,
        ) {
            if tx_data.eth_tx_hash != H256::zero() || tx_data.expiry > Self::time_now() {
                match Self::check_tx_status_on_ethereum(tx_id, tx_data.expiry, &author) {
                    EthTxStatus::Unresolved => {},
                    EthTxStatus::Succeeded => {
                        Self::provide_corroboration(tx_id, true, author);
                    },
                    EthTxStatus::Failed => {
                        Self::provide_corroboration(tx_id, false, author);
                    },
                }
            }
        }

        fn check_tx_status_on_ethereum(tx_id: u32, expiry: u64, author: &Author<T>) -> EthTxStatus {
            if let Ok(calldata) = Self::generate_corroboration_check_calldata(tx_id, expiry) {
                if let Ok(result) = Self::make_contract_view_call(calldata, &author) {
                    match result {
                        0 => return EthTxStatus::Unresolved,
                        1 => return EthTxStatus::Succeeded,
                        -1 => return EthTxStatus::Failed,
                        _ => {
                            log::error!(
                                "Invalid ethereum check response for tx_id {} and expiry {}: {}",
                                tx_id,
                                expiry,
                                result
                            );
                            return EthTxStatus::Unresolved
                        },
                    }
                }
            }

            log::error!("Invalid calldata generation for tx_id {} and expiry {}", tx_id, expiry);
            EthTxStatus::Unresolved
        }

        fn generate_corroboration_check_calldata(
            tx_id: u32,
            expiry: u64,
        ) -> Result<Vec<u8>, Error<T>> {
            let params = vec![
                (UINT32.to_vec(), tx_id.to_string().into_bytes()),
                (UINT256.to_vec(), expiry.to_string().into_bytes()),
            ];

            Self::encode_eth_function_input(&"corroborate".to_string(), &params)
        }

        fn provide_corroboration(tx_id: u32, tx_succeeded: bool, author: Author<T>) {
            let proof = Self::encode_add_corroboration_proof(tx_id, tx_succeeded, author.clone());
            let signature = author.key.sign(&proof).expect("Error signing proof");
            let call = Call::<T>::add_corroboration { tx_id, tx_succeeded, author, signature };
            let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
        }

        fn encode_add_corroboration_proof(
            tx_id: u32,
            tx_succeeded: bool,
            author: Author<T>,
        ) -> Vec<u8> {
            (ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, author.account_id).encode()
        }

        fn send_transaction_to_ethereum(tx_id: u32, author: Author<T>) {
            match Self::generate_send_transaction_calldata(tx_id) {
                Ok(calldata) => match Self::make_contract_send_call(calldata, &author) {
                    Ok(eth_tx_hash) => {
                        Self::provide_receipt(tx_id, eth_tx_hash, author);
                    },
                    Err(err) => {
                        log::error!("❌ Error calling AVN bridge contract: {:?}", err);
                    },
                },
                Err(err) => {
                    log::error!("❌ Error generating transaction calldata: {:?}", err);
                },
            }
        }

        fn provide_receipt(tx_id: u32, eth_tx_hash: H256, author: Author<T>) {
            let proof = Self::encode_add_receipt_proof(tx_id, eth_tx_hash, author.clone());
            let signature = author.key.sign(&proof).expect("Error signing proof");
            let call = Call::<T>::add_receipt { tx_id, eth_tx_hash, author, signature };
            let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
        }

        fn encode_add_receipt_proof(tx_id: u32, eth_tx_hash: H256, author: Author<T>) -> Vec<u8> {
            (ADD_RECEIPT_CONTEXT, tx_id, eth_tx_hash, author.account_id).encode()
        }

        fn generate_send_transaction_calldata(tx_id: u32) -> Result<Vec<u8>, Error<T>> {
            let tx_data = Transactions::<T>::get(tx_id).unwrap();

            let concatenated_confirmations =
                tx_data.confirmations.iter().fold(Vec::new(), |mut acc, conf| {
                    acc.extend_from_slice(conf.as_ref());
                    acc
                });

            let mut full_params = Self::unbound_params(tx_data.params);
            full_params.push((UINT256.to_vec(), tx_data.expiry.to_string().into_bytes()));
            full_params.push((UINT32.to_vec(), tx_id.to_string().into_bytes()));
            full_params.push((BYTES.to_vec(), concatenated_confirmations));

            let function_name = String::from_utf8(tx_data.function_name.into()).unwrap();

            Self::encode_eth_function_input(&function_name, &full_params)
        }

        fn encode_eth_function_input(
            function_name: &str,
            params: &[(Vec<u8>, Vec<u8>)],
        ) -> Result<Vec<u8>, Error<T>> {
            let inputs = params
                .iter()
                .filter_map(|(type_bytes, _)| {
                    Self::to_param_type(type_bytes).map(|kind| Param { name: "".to_string(), kind })
                })
                .collect::<Vec<_>>();

            let tokens: Result<Vec<_>, _> = params
                .iter()
                .map(|(type_bytes, value_bytes)| {
                    let param_type = Self::to_param_type(type_bytes)
                        .ok_or_else(|| Error::<T>::ParamTypeEncodingError)?;
                    Self::to_token_type(&param_type, value_bytes)
                })
                .collect();

            let function = Function {
                name: function_name.to_string(),
                inputs,
                outputs: Vec::<Param>::new(),
                constant: false,
            };

            function.encode_input(&tokens?).map_err(|_| Error::<T>::FunctionEncodingError)
        }

        fn to_param_type(key: &Vec<u8>) -> Option<ParamType> {
            match key.as_slice() {
                UINT256 => Some(ParamType::Uint(256)),
                UINT128 => Some(ParamType::Uint(128)),
                UINT32 => Some(ParamType::Uint(32)),
                BYTES => Some(ParamType::Bytes),
                BYTES32 => Some(ParamType::FixedBytes(32)),
                _ => None,
            }
        }

        fn to_token_type(kind: &ParamType, value: &Vec<u8>) -> Result<Token, Error<T>> {
            match kind {
                ParamType::Uint(_) => {
                    let dec_value = Int::from_dec_str(&String::from_utf8(value.clone()).unwrap())
                        .map_err(|_| Error::<T>::InvalidUint)?;
                    Ok(Token::Uint(dec_value))
                },
                ParamType::Bytes => Ok(Token::Bytes(value.clone())),
                ParamType::FixedBytes(size) => {
                    if value.len() != *size {
                        return Err(Error::<T>::InvalidBytes)
                    }
                    Ok(Token::FixedBytes(value.clone()))
                },
                _ => Err(Error::<T>::InvalidData),
            }
        }

        fn make_contract_send_call(
            calldata: Vec<u8>,
            author: &Author<T>,
        ) -> Result<H256, DispatchError> {
            Self::execute_contract_call(calldata, author, "send", Self::process_send_response)
        }

        fn make_contract_view_call(
            calldata: Vec<u8>,
            author: &Author<T>,
        ) -> Result<i8, DispatchError> {
            Self::execute_contract_call(calldata, author, "view", Self::process_view_response)
        }

        fn execute_contract_call<R>(
            calldata: Vec<u8>,
            author: &Author<T>,
            endpoint: &str,
            process_response: fn(Vec<u8>) -> Result<R, DispatchError>,
        ) -> Result<R, DispatchError> {
            // TODO: replace with AVN pallet's get_bridge_contract_address()
            let contract_address = H160(hex!("F05Df39f745A240fb133cC4a11E42467FAB10f1F"));
            let sender = T::AccountToBytesConvert::into_bytes(&author.account_id);
            let transaction_to_send = EthTransaction::new(sender, contract_address, calldata);

            let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
            let external_service_port_number = AVN::<T>::get_external_service_port_number();

            let url = format!("http://127.0.0.1:{}/eth/{}", external_service_port_number, endpoint);

            let pending = http::Request::default()
                .deadline(deadline)
                .method(http::Method::Post)
                .url(&url)
                .body(vec![transaction_to_send.encode()])
                .send()
                .map_err(|_| Error::<T>::RequestTimedOut)?;

            let response = pending
                .try_wait(deadline)
                .map_err(|_| Error::<T>::DeadlineReached)?
                .map_err(|_| Error::<T>::DeadlineReached)?;

            if response.code != 200 {
                log::error!("❌ Unexpected status code: {}", response.code);
                return Err(Error::<T>::UnexpectedStatusCode)?
            }

            let result: Vec<u8> = response.body().collect::<Vec<u8>>();

            process_response(result)
        }

        fn process_send_response(result: Vec<u8>) -> Result<H256, DispatchError> {
            if result.len() != 64 {
                log::error!("❌ Ethereum transaction hash is not valid: {:?}", result);
                return Err(Error::<T>::InvalidHashLength.into())
            }

            let tx_hash_string = core::str::from_utf8(&result).map_err(|e| {
                log::error!("❌ Error converting txHash bytes to string: {:?}", e);
                Error::<T>::InvalidUTF8Bytes
            })?;

            let mut data: [u8; 32] = [0; 32];
            hex::decode_to_slice(tx_hash_string, &mut data[..])
                .map_err(|_| Error::<T>::InvalidHexString)?;

            Ok(H256::from_slice(&data))
        }

        fn process_view_response(result: Vec<u8>) -> Result<i8, DispatchError> {
            if result.len() != 1 {
                log::error!("❌ Invalid data length for int8: {:?}", result);
                return Err(Error::<T>::InvalidDataLength.into())
            }

            Ok(result[0] as i8)
        }

        fn finalize_state(tx_id: u32, tx_succeeded: bool) -> Result<(), DispatchError> {
            // Alert the originating pallet first:
            T::HandleAvnBridgeResult::result(tx_id, tx_succeeded)
                .map_err(|_| Error::<T>::HandleAvnBridgeResultFailed)?;

            let mut tx_data = Transactions::<T>::get(tx_id).ok_or(Error::<T>::TxIdNotFound)?;
            tx_data.status =
                if tx_succeeded { EthTxStatus::Succeeded } else { EthTxStatus::Failed };
            Self::remove_from_unresolved_tx_list(tx_id)?;
            Corroborations::<T>::remove(tx_id);
            <Transactions<T>>::insert(tx_id, tx_data);
            Ok(())
        }
    }
}
