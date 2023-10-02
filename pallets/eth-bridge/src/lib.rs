// Copyright 2023 Aventus Network Services (UK) Ltd.

#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};
use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{Function, Int, Param, ParamType, Token};
use frame_support::{dispatch::DispatchResultWithPostInfo, traits::IsSubType, BoundedVec};
use frame_system::{ensure_none, ensure_root, pallet_prelude::OriginFor};
use hex_literal::hex;
use pallet_avn::{self as avn};
use sp_avn_common::EthTransaction;
use sp_core::{H160, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::traits::Dispatchable;
use sp_avn_common::calculate_two_third_quorum;

pub use pallet::*;
pub mod default_weights;
pub use default_weights::WeightInfo;

pub type AVN<T> = avn::Pallet<T>;

// TODO: Should we enable all Ethereum types (here and in to_token_type() and to_param_type()) from the outset?
const UINT256: &[u8] = b"uint256";
const UINT128: &[u8] = b"uint128";
const UINT32: &[u8] = b"uint32";
const BYTES: &[u8] = b"bytes";
const BYTES32: &[u8] = b"bytes32";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::UnixTime, Blake2_128Concat};

    #[pallet::config]
    pub trait Config: frame_system::Config + avn::Config {
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as frame_system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type TimeProvider: UnixTime;
        type WeightInfo: WeightInfo;

        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>
            + IsSubType<Call<Self>>
            + From<Call<Self>>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PublishToEthereum { tx_id: u32, function_name: Vec<u8>, params: Vec<(Vec<u8>, Vec<u8>)> },
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn get_next_tx_id)]
    pub type NextTxId<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_eth_tx_lifetime_secs)]
    pub type TimeoutDuration<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub _phantom: sp_std::marker::PhantomData<T>,
        pub tx_lifetime_secs: u64,
        pub next_tx_id: u32,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { _phantom: Default::default(), tx_lifetime_secs: 0, next_tx_id: 0 }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            TimeoutDuration::<T>::put(self.tx_lifetime_secs);
            NextTxId::<T>::put(self.next_tx_id);
        }
    }

    #[pallet::error]
    pub enum Error<T> {
        TxIdNotFound,
        DuplicateConfirmation,
        ExceedsConfirmationLimit,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::set_eth_tx_lifetime_secs())]
        pub fn set_eth_tx_lifetime_secs(
            origin: OriginFor<T>,
            tx_lifetime_secs: u64,
        ) -> DispatchResultWithPostInfo {
            // Sudo only
            ensure_root(origin)?;
            TimeoutDuration::<T>::put(tx_lifetime_secs);
            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10_000)] // TODO: set weight
        pub fn add_confirmation(
            origin: OriginFor<T>,
            tx_id: u32,
            confirmation: [u8; 65],
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
    
            Transactions::<T>::try_mutate_exists(tx_id, |maybe_tx_data| -> DispatchResultWithPostInfo {
                let tx_data = maybe_tx_data.as_mut().ok_or(Error::<T>::TxIdNotFound)?;
    
                if tx_data.confirmations.iter().any(|&conf| conf == confirmation) {
                    return Err(Error::<T>::DuplicateConfirmation.into());
                }
    
                tx_data.confirmations.try_push(confirmation).map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;

                if tx_data.confirmations.len() >= calculate_two_third_quorum(AVN::<T>::validators().len() as u32).try_into().unwrap() {
                    // TODO: Get chosen author
                    let author: [u8; 32] = [0u8; 32];
                    tx_data.author = Some(author);
                }

                Ok(().into())
            })
        }
    }

    #[pallet::storage]
    #[pallet::getter(fn get_transaction_data)]
    pub type Transactions<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, TransactionData, ValueQuery>;

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub enum TransactionStatus {
        #[default]
        Unsettled,
        Succeeded,
        Failed,
    }

    pub type FunctionLimit = ConstU32<32>; // Max chars in T1 function name
    pub type ParamsLimit = ConstU32<5>; // Max params (not including expiry, t2TxId, confirmations)
    pub type TypeLimit = ConstU32<7>; // Max chars in a type name
    pub type ValueLimit = ConstU32<130>; // Max chars in a value
    pub type ConfirmationsLimit = ConstU32<1000>; // Max Confirmations - TODO: Review this

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub struct TransactionData {
        pub function_name: BoundedVec<u8, FunctionLimit>,
        pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
        pub expiry: u64,
        pub msg_hash: H256,
        pub confirmations: BoundedVec<[u8; 65], ConfirmationsLimit>,
        pub author: Option<[u8; 32]>,
        pub eth_tx_hash: H256,
        pub status: TransactionStatus,
    }

    impl<T: Config> Pallet<T> {
        pub fn publish_to_ethereum(
            function_name: Vec<u8>,
            params: Vec<(Vec<u8>, Vec<u8>)>,
        ) -> Result<u32, ethabi::Error> {
            let expiry = T::TimeProvider::now().as_secs() + Self::get_eth_tx_lifetime_secs();
            let tx_id = Self::get_and_update_next_tx_id();

            Self::deposit_event(Event::<T>::PublishToEthereum {
                tx_id,
                function_name: function_name.clone(),
                params: params.clone(),
            });

            let mut extended_params = params.clone();
            extended_params.push((UINT256.to_vec(), expiry.to_string().into_bytes()));
            extended_params.push((UINT32.to_vec(), tx_id.to_string().into_bytes()));
            let msg_hash = Self::generate_msg_hash(&extended_params)?;

            let tx_data = TransactionData {
                function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name).unwrap(),
                params: Self::bound_params(params).unwrap(),
                expiry,
                msg_hash,
                confirmations: BoundedVec::<[u8; 65], ConfirmationsLimit>::default(),
                author: None,
                eth_tx_hash: H256::zero(),
                status: TransactionStatus::Unsettled,
            };

            <Transactions<T>>::insert(tx_id, tx_data);

            Ok(tx_id)
        }

        fn bound_params(
            params: Vec<(Vec<u8>, Vec<u8>)>,
        ) -> Result<
            BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
            DispatchError,
        > {
            let result: Result<Vec<_>, _> = params
                .into_iter()
                .map(|(type_vec, value_vec)| {
                    let type_bounded = BoundedVec::try_from(type_vec)
                        .map_err(|_| DispatchError::Other("Type name length"))?;
                    let value_bounded = BoundedVec::try_from(value_vec)
                        .map_err(|_| DispatchError::Other("Value length"))?;
                    Ok::<_, DispatchError>((type_bounded, value_bounded))
                })
                .collect();

            BoundedVec::<_, ParamsLimit>::try_from(result?)
                .map_err(|_| DispatchError::Other("Number of params"))
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

        fn get_and_update_next_tx_id() -> u32 {
            let tx_id = NextTxId::<T>::get();
            NextTxId::<T>::put(tx_id + 1);
            tx_id
        }

        fn generate_msg_hash(params: &Vec<(Vec<u8>, Vec<u8>)>) -> Result<H256, ethabi::Error> {
            let (types, values): (Vec<&Vec<u8>>, Vec<&Vec<u8>>) =
                params.iter().map(|(a, b)| (a, b)).unzip();

            let types: Result<Vec<ParamType>, _> = types
                .iter()
                .map(|s| Self::to_param_type(s).ok_or(ethabi::Error::InvalidName(hex::encode(s))))
                .collect();
            let types = types?;

            let tokens: Result<Vec<Token>, _> = types
                .iter()
                .zip(values.iter())
                .map(|(kind, value)| Self::to_token_type(kind, value))
                .collect();
            let tokens = tokens?;

            let encoded = ethabi::encode(&tokens);
            let msg_hash = keccak_256(&encoded);

            Ok(H256::from(msg_hash))
        }

        fn generate_calldata(tx_id: u32) -> Result<Vec<u8>, ethabi::Error> {
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

            let function = Function {
                name: String::from_utf8(tx_data.function_name.clone().into())
                    .unwrap_or_else(|_| "Invalid function name".to_string()),
                inputs: full_params
                    .iter()
                    .map(|(type_bytes, _)| Param {
                        name: "".to_string(),
                        kind: Self::to_param_type(type_bytes).unwrap(),
                    })
                    .collect(),
                outputs: Vec::<Param>::new(),
                constant: false,
            };

            let tokens: Result<Vec<Token>, _> = full_params
                .iter()
                .map(|(type_bytes, value_bytes)| {
                    Self::to_token_type(&Self::to_param_type(type_bytes).unwrap(), value_bytes)
                })
                .collect();

            function.encode_input(&tokens?)
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

        fn to_token_type(kind: &ParamType, value: &Vec<u8>) -> Result<Token, ethabi::Error> {
            match kind {
                ParamType::Uint(_) => {
                    let dec_value = Int::from_dec_str(&String::from_utf8(value.clone()).unwrap())
                        .map_err(|_| ethabi::Error::InvalidData)?;
                    Ok(Token::Uint(dec_value))
                },
                ParamType::Bytes => Ok(Token::Bytes(value.clone())),
                ParamType::FixedBytes(size) => {
                    if value.len() != *size {
                        return Err(ethabi::Error::InvalidData)
                    }
                    Ok(Token::FixedBytes(value.clone()))
                },
                _ => Err(ethabi::Error::InvalidData),
            }
        }

        pub fn generate_eth_transaction(tx_id: u32) -> Result<EthTransaction, ethabi::Error> {
            // TODO: CHECK CONFIRMATIONS > QUORUM
            // TODO: Get chosen sender:
            let author: [u8; 32] = [0u8; 32];
            // TODO: Replace with AVN bridge contract getter and remove H160 and hex:
            let bridge_contract = H160(hex!("F05Df39f745A240fb133cC4a11E42467FAB10f1F"));

            let mut tx_data = Transactions::<T>::get(tx_id);
            tx_data.author = Some(author);
            <Transactions<T>>::insert(tx_id, tx_data);

            let calldata = Self::generate_calldata(tx_id)?;

            Ok(EthTransaction::new(author, bridge_contract, calldata))
        }
    }
}

#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;

#[cfg(test)]
#[path = "tests/tests.rs"]
pub mod tests;
