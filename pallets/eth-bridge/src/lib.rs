// Copyright 2023 Aventus Network Services (UK) Ltd.

//! This pallet provides a single interface, "publish_to_avn_bridge", which enables other
//! pallets to execute any author-accessible function on the Ethereum-based "avn-bridge" contract.
//! To do so, callers pass the desired avn-bridge function name, along with an array of
//! parameter tuples, each comprising the data type and its corresponding value.
//! Upon receipt of a request, this pallet takes charge of the entire transaction process.
//! This culminates in the conclusive determination of the transaction's status on Ethereum,
//! and the emission of an event the originating pallet can use to determine whether to
//! commit or rollback its state.
//!
//! Specifically, the pallet manages:
//!
//! - The packaging and encoding of the transaction to ensure Ethereum compatibility.
//!
//! - The addition of a timestamp, delineating the deadline by which the transaction must reach
//!   the contract.
//!
//! - The addition of a unique transaction ID, against which request data can be stored on the
//!   AvN and the transaction status in the avn-bridge contract can later be checked.
//!
//! - Collection of the necessary ECDSA signatures, labelled "confirmations", which serve to
//!   prove the AvN consensus for the transaction to the avn-bridge.
//!
//! - Appointing a designated author responsible for sending the transaction to Ethereum.
//!
//! - Utilising the transaction ID and expiry to determine the status of a sent transaction and
//!   provide consensus via "corroborations".
//!
//! - Alerting the originating pallet to the final outcome via a callback.
//!
//! To enable these operations, an off-chain worker continuously monitors all transactions with
//! as yet unresolved statuses. It actively aggregates either confirmations or corroborations as
//! required, via their respective unsigned extrinsic re-entry methods.

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
use sp_core::{H160, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::{
    offchain::{http, Duration},
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

pub const PALLET_NAME: &'static [u8] = b"EthBridge";
pub const ADD_CONFIRMATION_CONTEXT: &'static [u8] = b"EthBridgeConfirmation";
pub const ADD_CORROBORATION_CONTEXT: &'static [u8] = b"EthBridgeCorroboration";

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
        frame_system::Config + avn::Config + SendTransactionTypes<Call<Self>>
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
        ErrorCalculatingSender,
        ExceedsConfirmationLimit,
        ExceedsFunctionNameLimit,
        FunctionEncodingError,
        FunctionNameError,
        InvalidBytes,
        InvalidData,
        InvalidDataLength,
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
        #[pallet::weight(<T as Config>::WeightInfo::add_confirmation(CONFIRMATIONS_LIMIT))]
        pub fn add_confirmation(
            origin: OriginFor<T>,
            tx_id: u32,
            confirmation: [u8; 65],
            _author: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            // TODO: Add Eth sig checking

            Transactions::<T>::try_mutate_exists(
                tx_id,
                |maybe_tx_data| -> DispatchResultWithPostInfo {
                    let tx_data = maybe_tx_data.as_mut().ok_or(Error::<T>::TxIdNotFound)?;

                    if tx_data.confirmations.iter().any(|&conf| conf == confirmation) {
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
        #[pallet::weight(<T as Config>::WeightInfo::add_corroboration())]
        pub fn add_corroboration(
            origin: OriginFor<T>,
            tx_id: u32,
            succeeded: bool,
            author: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            if !UnresolvedTxList::<T>::get().contains(&tx_id) {
                return Ok(().into())
            }

            let author_account_id =
                Some(T::AccountToBytesConvert::into_bytes(&author.account_id)).unwrap();
            let mut corroborations = Corroborations::<T>::get(tx_id);
            let mut tx_data = Transactions::<T>::get(tx_id);

            if succeeded {
                if !corroborations.success.contains(&author_account_id) {
                    let mut tmp_vec = Vec::new();
                    tmp_vec.push(author_account_id);
                    corroborations
                        .success
                        .try_append(&mut tmp_vec)
                        .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;
                }

                if Self::consensus_is_reached(corroborations.success.len()) {
                    tx_data.status = EthTxStatus::Succeeded;
                    // TODO: run COMMIT callback here
                    Self::remove_from_unresolved_tx_list(tx_id)?;
                    Corroborations::<T>::remove(tx_id);
                }
            } else {
                if !corroborations.failure.contains(&author_account_id) {
                    let mut tmp_vec = Vec::new();
                    tmp_vec.push(author_account_id);
                    corroborations
                        .failure
                        .try_append(&mut tmp_vec)
                        .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;
                }

                if Self::consensus_is_reached(corroborations.failure.len()) {
                    tx_data.status = EthTxStatus::Failed;
                    // TODO: run ROLLBACK callback here
                    Self::remove_from_unresolved_tx_list(tx_id)?;
                    Corroborations::<T>::remove(tx_id);
                }
            }

            <Transactions<T>>::insert(tx_id, tx_data);
            <Corroborations<T>>::insert(tx_id, corroborations);

            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn offchain_worker(block_number: T::BlockNumber) {
            if let Ok(author) = setup_ocw::<T>(block_number) {
                for tx_id in UnresolvedTxList::<T>::get() {
                    Self::process_unresolved_transaction(tx_id, author.clone(), block_number);
                }
            }
        }
    }

    fn setup_ocw<T: Config>(
        block_number: T::BlockNumber,
    ) -> Result<Validator<<T as avn::Config>::AuthorityId, T::AccountId>, DispatchError> {
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
                        &(ADD_CONFIRMATION_CONTEXT, tx_id.clone(), confirmation, author),
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
                Call::add_corroboration { tx_id, succeeded, author, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(ADD_CORROBORATION_CONTEXT, tx_id.clone(), succeeded, author),
                        &author,
                        signature,
                    ) {
                        ValidTransaction::with_tag_prefix("EthBridgeAddCorroboration")
                            .and_provides((call, tx_id))
                            .priority(TransactionPriority::max_value())
                            .build()
                    } else {
                        InvalidTransaction::Custom(2u8).into()
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
        StorageMap<_, Blake2_128Concat, u32, TransactionData, ValueQuery>;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct TransactionData {
        pub function_name: BoundedVec<u8, FunctionLimit>,
        pub params:
            BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
        pub expiry: u64,
        pub msg_hash: H256,
        pub confirmations: BoundedVec<[u8; 65], ConfirmationsLimit>,
        pub sender: Option<[u8; 32]>,
        pub eth_tx_hash: H256,
        pub status: EthTxStatus,
    }

    #[pallet::storage]
    #[pallet::getter(fn get_corroboration_data)]
    pub type Corroborations<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, CorroborationData, ValueQuery>;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct CorroborationData {
        pub success: BoundedVec<[u8; 32], ConfirmationsLimit>,
        pub failure: BoundedVec<[u8; 32], ConfirmationsLimit>,
    }

    impl<T: Config> Pallet<T> {
        pub fn publish_to_avn_bridge(
            function_name: &[u8],
            params: &[(Vec<u8>, Vec<u8>)],
        ) -> Result<u32, Error<T>> {
            let expiry = <T as pallet::Config>::TimeProvider::now().as_secs() +
                Self::get_eth_tx_lifetime_secs();
            let tx_id = Self::use_next_tx_id();

            let mut extended_params = params.to_vec();
            extended_params.push((UINT256.to_vec(), expiry.to_string().into_bytes()));
            extended_params.push((UINT32.to_vec(), tx_id.to_string().into_bytes()));
            let msg_hash = Self::create_msg_hash(&extended_params)?;

            let tx_data = TransactionData {
                function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec())
                    .map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
                params: Self::bound_params(params.to_vec())?,
                expiry,
                msg_hash,
                confirmations: BoundedVec::<[u8; 65], ConfirmationsLimit>::default(),
                sender: None,
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
        fn process_unresolved_transaction(
            tx_id: u32,
            author: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            block_number: T::BlockNumber,
        ) {
            let author_account_id = T::AccountToBytesConvert::into_bytes(&author.account_id);
            let sender = match Self::get_or_assign_sender(tx_id, block_number) {
                Some(sender) => sender,
                None => return,
            };
            let self_is_sender = author_account_id == sender;
            let tx_data = Transactions::<T>::get(tx_id);

            if Self::consensus_is_reached(tx_data.confirmations.len()) {
                if self_is_sender {
                    Self::send_transaction_to_ethereum(tx_id, tx_data, author_account_id);
                } else {
                    Self::attempt_to_confirm_eth_tx_status(
                        tx_id,
                        tx_data,
                        author,
                        author_account_id,
                    );
                }
            } else if !self_is_sender {
                // The sender's confirmation is implicit so we only collect the others
                Self::provide_signed_confirmation(tx_id, tx_data, author, author_account_id);
            }
        }

        fn use_next_tx_id() -> u32 {
            let tx_id = NextTxId::<T>::get();
            NextTxId::<T>::put(tx_id + 1);
            tx_id
        }

        fn consensus_is_reached(entries: usize) -> bool {
            let quorum = calculate_one_third_quorum(AVN::<T>::validators().len() as u32);
            entries as u32 >= quorum
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

        fn get_or_assign_sender(tx_id: u32, block_number: T::BlockNumber) -> Option<[u8; 32]> {
            let mut tx_data = Transactions::<T>::get(tx_id);
            match tx_data.sender {
                Some(sender) => Some(sender),
                None => match AVN::<T>::calculate_primary_validator(block_number) {
                    Ok(primary_validator) => {
                        let sender = T::AccountToBytesConvert::into_bytes(&primary_validator);
                        tx_data.sender = Some(sender.clone());
                        <Transactions<T>>::insert(tx_id, tx_data);
                        Some(sender)
                    },
                    Err(_) => {
                        log::error!("❌ Error choosing sender.");
                        None
                    },
                },
            }
        }

        fn create_msg_hash(params: &[(Vec<u8>, Vec<u8>)]) -> Result<H256, Error<T>> {
            let (types, values): (Vec<_>, Vec<_>) = params.iter().cloned().unzip();

            let types = types
                .iter()
                .map(|s| Self::to_param_type(s).ok_or_else(|| Error::<T>::MsgHashError))
                .collect::<Result<Vec<_>, _>>()?;

            let tokens = types
                .into_iter()
                .zip(values.iter())
                .map(|(kind, value)| Self::to_token_type(&kind, value))
                .collect::<Result<Vec<_>, _>>()?;

            let encoded = ethabi::encode(&tokens);
            let msg_hash = keccak_256(&encoded);

            Ok(H256::from(msg_hash))
        }

        fn provide_signed_confirmation(
            tx_id: u32,
            tx_data: TransactionData,
            author: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            author_account_id: [u8; 32],
        ) {
            match Self::sign_msg_hash(tx_data.msg_hash) {
                Ok(confirmation) => {
                    let proof =
                        Self::encode_add_confirmation_proof(tx_id, confirmation, author_account_id);
                    let signature = author.key.sign(&proof).expect("Error signing proof");
                    let call = Call::<T>::add_confirmation {
                        tx_id,
                        confirmation,
                        author: author.clone(),
                        signature,
                    };
                    let _ =
                        SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
                },
                Err(err) => {
                    log::error!("❌ Error signing confirmation: {:?}", err);
                },
            }
        }

        fn sign_msg_hash(msg_hash: H256) -> Result<[u8; 65], DispatchError> {
            let msg_hash_string = msg_hash.to_string();
            let signature =
                AVN::<T>::request_ecdsa_signature_from_external_service(&msg_hash_string)?;
            let signature_bytes: [u8; 65] = signature.into();
            Ok(signature_bytes)
        }

        fn encode_add_confirmation_proof(
            tx_id: u32,
            confirmation: [u8; 65],
            author_account_id: [u8; 32],
        ) -> Vec<u8> {
            return (
                ADD_CONFIRMATION_CONTEXT,
                tx_id.clone(),
                confirmation,
                author_account_id.clone(),
            )
                .encode()
        }

        fn attempt_to_confirm_eth_tx_status(
            tx_id: u32,
            tx_data: TransactionData,
            author: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            author_account_id: [u8; 32],
        ) {
            if tx_data.eth_tx_hash != H256::zero() {
                match Self::check_tx_status_on_ethereum(tx_id, tx_data.expiry, author_account_id) {
                    EthTxStatus::Unresolved => {},
                    EthTxStatus::Succeeded => {
                        Self::provide_corroboration(tx_id, true, &author, author_account_id);
                    },
                    EthTxStatus::Failed => {
                        Self::provide_corroboration(tx_id, false, &author, author_account_id);
                    },
                }
            }
        }

        fn check_tx_status_on_ethereum(
            tx_id: u32,
            expiry: u64,
            author_account_id: [u8; 32],
        ) -> EthTxStatus {
            if let Ok(calldata) = Self::generate_corroboration_check_calldata(tx_id, expiry) {
                if let Ok(result) =
                    Self::call_avn_bridge_contract_view_method(calldata, author_account_id)
                {
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

        fn provide_corroboration(
            tx_id: u32,
            succeeded: bool,
            author: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            author_account_id: [u8; 32],
        ) {
            let proof = Self::encode_add_corroboration_proof(tx_id, succeeded, author_account_id);
            let signature = author.key.sign(&proof).expect("Error signing proof");
            let call = Call::<T>::add_corroboration {
                tx_id,
                succeeded,
                author: author.clone(),
                signature,
            };
            let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
        }

        fn encode_add_corroboration_proof(
            tx_id: u32,
            succeeded: bool,
            author_account_id: [u8; 32],
        ) -> Vec<u8> {
            return (ADD_CORROBORATION_CONTEXT, tx_id.clone(), succeeded, author_account_id.clone())
                .encode()
        }

        fn send_transaction_to_ethereum(
            tx_id: u32,
            mut tx_data: TransactionData,
            author_account_id: [u8; 32],
        ) {
            match Self::generate_send_transaction_calldata(tx_id) {
                Ok(calldata) =>
                    match Self::call_avn_bridge_contract_send_method(calldata, author_account_id) {
                        Ok(eth_tx_hash) => {
                            tx_data.eth_tx_hash = eth_tx_hash;
                            <Transactions<T>>::insert(tx_id, tx_data);
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

        // TODO: Make function private and pass TransactionData in once tests are configured to
        // trigger OCW
        pub fn generate_send_transaction_calldata(tx_id: u32) -> Result<Vec<u8>, Error<T>> {
            let tx_data = Transactions::<T>::get(tx_id);

            let concatenated_confirmations =
                tx_data.confirmations.iter().fold(Vec::new(), |mut acc, conf| {
                    acc.extend_from_slice(conf);
                    acc
                });

            let mut full_params = Self::unbound_params(tx_data.params.clone());
            full_params.push((UINT256.to_vec(), tx_data.expiry.to_string().into_bytes()));
            full_params.push((UINT32.to_vec(), tx_id.to_string().into_bytes()));
            full_params.push((BYTES.to_vec(), concatenated_confirmations));

            let function_name = String::from_utf8(tx_data.function_name.clone().into())
                .map_err(|_| Error::<T>::FunctionNameError)?;

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

        fn call_avn_bridge_contract_send_method(
            calldata: Vec<u8>,
            author_account_id: [u8; 32],
        ) -> Result<H256, DispatchError> {
            Self::execute_avn_bridge_request(
                calldata,
                author_account_id,
                "send",
                Self::process_send_response,
            )
        }

        fn call_avn_bridge_contract_view_method(
            calldata: Vec<u8>,
            author_account_id: [u8; 32],
        ) -> Result<i8, DispatchError> {
            Self::execute_avn_bridge_request(
                calldata,
                author_account_id,
                "view",
                Self::process_view_response,
            )
        }

        fn execute_avn_bridge_request<R>(
            calldata: Vec<u8>,
            author_account_id: [u8; 32],
            endpoint: &str,
            process_response: fn(Vec<u8>) -> Result<R, DispatchError>,
        ) -> Result<R, DispatchError> {
            // TODO: replace with AVN pallet's get_bridge_contract_address()
            let contract_address = H160(hex!("F05Df39f745A240fb133cC4a11E42467FAB10f1F"));
            let transaction_to_send =
                EthTransaction::new(author_account_id, contract_address, calldata);

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
                return Err(Error::<T>::InvalidHashLength)?
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
                return Err(Error::<T>::InvalidDataLength)?
            }

            Ok(result[0] as i8)
        }
    }
}
