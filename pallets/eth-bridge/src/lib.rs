// Copyright 2023 Aventus Network Services (UK) Ltd.

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
    ensure_none, ensure_root, ensure_signed,
    offchain::{SendTransactionTypes, SubmitTransaction},
    pallet_prelude::OriginFor,
};
use hex_literal::hex;
use pallet_avn::{self as avn};
use sp_avn_common::{calculate_one_third_quorum, EthTransaction};
use sp_core::{H160, H256};
use sp_io::hashing::keccak_256;
use sp_runtime::{
    offchain::{http, Duration},
    traits::Dispatchable,
};

pub use pallet::*;
pub mod default_weights;
pub use default_weights::WeightInfo;

pub type AVN<T> = avn::Pallet<T>;

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
    }

    // TODO: Pallet needs more events
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

    #[pallet::storage]
    pub type UnresolvedTxList<T: Config> =
        StorageValue<_, BoundedVec<u32, T::MaxUnresolvedTx>, ValueQuery>;

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
        DeadlineReached,
        DuplicateConfirmation,
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
            tx_lifetime_secs: u64,
        ) -> DispatchResultWithPostInfo {
            // SUDO
            ensure_root(origin)?;
            TimeoutDuration::<T>::put(tx_lifetime_secs);
            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10_000)] // TODO: set actual weight
        pub fn add_confirmation(
            origin: OriginFor<T>,
            tx_id: u32,
            confirmation: [u8; 65],
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

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
        #[pallet::weight(10_000)] // TODO: set actual weight
        pub fn add_corroboration(
            origin: OriginFor<T>,
            tx_id: u32,
            succeeded: bool,
        ) -> DispatchResultWithPostInfo {
            let author = ensure_signed(origin)?;
            let author: [u8; 32] =
                author.encode().try_into().expect("AccountId should be 32 bytes");

            if !UnresolvedTxList::<T>::get().contains(&tx_id) {
                return Ok(().into())
            }

            let mut corroborations = Corroborations::<T>::get(tx_id);
            let mut tx_data = Transactions::<T>::get(tx_id);

            if succeeded {
                if !corroborations.success.contains(&author) {
                    let mut tmp_vec = Vec::new();
                    tmp_vec.push(author);
                    corroborations
                        .success
                        .try_append(&mut tmp_vec)
                        .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;
                }

                if Self::quorum_is_reached(corroborations.success.len()) {
                    tx_data.state = EthTxState::Succeeded;
                    // TODO: run COMMIT callback here
                    Self::remove_from_unresolved(tx_id)?;
                    Corroborations::<T>::remove(tx_id);
                }
            } else {
                if !corroborations.failure.contains(&author) {
                    let mut tmp_vec = Vec::new();
                    tmp_vec.push(author);
                    corroborations
                        .failure
                        .try_append(&mut tmp_vec)
                        .map_err(|_| Error::<T>::ExceedsConfirmationLimit)?;
                }

                if Self::quorum_is_reached(corroborations.failure.len()) {
                    tx_data.state = EthTxState::Failed;
                    // TODO: run ROLLBACK callback here
                    Self::remove_from_unresolved(tx_id)?;
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
        // TODO: log errors
        fn offchain_worker(_block_number: T::BlockNumber) {
            // TODO: use actual self account
            let this_account: [u8; 32] = [0u8; 32];

            for tx_id in UnresolvedTxList::<T>::get() {

                let mut tx_data = Transactions::<T>::get(tx_id);
                let this_account_is_sender = tx_data.sending_author.unwrap() == this_account;

                if !this_account_is_sender && Self::quorum_is_reached(tx_data.confirmations.len()) {
                    let confirmation =
                        generate_signed_ethereum_confirmation::<T>(tx_data.msg_hash).unwrap();
                    let call = Call::<T>::add_confirmation { tx_id, confirmation };
                    let _ =
                        SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());

                } else if Self::quorum_is_reached(tx_data.confirmations.len()) {

                    if this_account_is_sender {

                        let calldata = Self::generate_transaction_calldata(tx_id).unwrap();
                        let eth_tx_hash: H256 = Self::call_avn_bridge_contract_send_method(calldata).unwrap();
                        tx_data.eth_tx_hash = eth_tx_hash;
                        <Transactions<T>>::insert(tx_id, tx_data);

                    } else if tx_data.eth_tx_hash != H256::zero() {

                        match Self::check_ethereum(tx_id, tx_data.expiry) {
                            EthTxState::Unresolved => {},
                            EthTxState::Succeeded => {
                                let call = Call::<T>::add_corroboration { tx_id, succeeded: true };
                                let _ =
                                    SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                                        call.into(),
                                    );
                            },
                            EthTxState::Failed => {
                                let call = Call::<T>::add_corroboration { tx_id, succeeded: false };
                                let _ =
                                    SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                                        call.into(),
                                    );
                            },
                        }
                    }
                }
            }
        }
    }

    fn generate_signed_ethereum_confirmation<T: Config>(
        msg_hash: H256,
    ) -> Result<[u8; 65], DispatchError> {
        let msg_hash_string = msg_hash.to_string();
        let signature = AVN::<T>::request_ecdsa_signature_from_external_service(&msg_hash_string)?;
        let signature_bytes: [u8; 65] = signature.into();
        Ok(signature_bytes)
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                Call::add_confirmation { tx_id, confirmation: _ } =>
                    ValidTransaction::with_tag_prefix("EthBridgeAddConfirmation")
                        .and_provides((call, tx_id))
                        .priority(TransactionPriority::max_value())
                        .build(),
                Call::add_corroboration { tx_id, succeeded: _ } =>
                    ValidTransaction::with_tag_prefix("EthBridgeAddCorroboration")
                        .and_provides((call, tx_id))
                        .priority(TransactionPriority::max_value())
                        .build(),
                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
    pub enum EthTxState {
        #[default]
        Unresolved,
        Succeeded,
        Failed,
    }

    pub type FunctionLimit = ConstU32<32>; // Max chars in T1 function name
    pub type ParamsLimit = ConstU32<5>; // Max params (not including expiry, t2TxId, confirmations)
    pub type TypeLimit = ConstU32<7>; // Max chars in a T1 type name
    pub type ValueLimit = ConstU32<130>; // Max chars in a value
    pub type ConfirmationsLimit = ConstU32<100>; // Confirmations/corroborations limit

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
        pub sending_author: Option<[u8; 32]>,
        pub eth_tx_hash: H256,
        pub state: EthTxState,
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
        pub fn publish_to_ethereum(
            function_name: &[u8],
            params: &[(Vec<u8>, Vec<u8>)],
        ) -> Result<u32, Error<T>> {
            let expiry = <T as pallet::Config>::TimeProvider::now().as_secs() + Self::get_eth_tx_lifetime_secs();
            let tx_id = Self::get_and_update_next_tx_id();
        
            Self::deposit_event(Event::<T>::PublishToEthereum {
                tx_id,
                function_name: function_name.to_vec(),
                params: params.to_vec(),
            });
        
            let mut extended_params = params.to_vec();
            extended_params.push((UINT256.to_vec(), expiry.to_string().into_bytes()));
            extended_params.push((UINT32.to_vec(), tx_id.to_string().into_bytes()));
            let msg_hash = Self::create_msg_hash(&extended_params)?;
        
            let tx_data = TransactionData {
                function_name: BoundedVec::<u8, FunctionLimit>::try_from(function_name.to_vec()).map_err(|_| Error::<T>::ExceedsFunctionNameLimit)?,
                params: Self::bound_params(params.to_vec())?,
                expiry,
                msg_hash,
                confirmations: BoundedVec::<[u8; 65], ConfirmationsLimit>::default(),
                sending_author: None,
                eth_tx_hash: H256::zero(),
                state: EthTxState::Unresolved,
            };
        
            let corroborations = CorroborationData {
                success: BoundedVec::default(),
                failure: BoundedVec::default(),
            };
        
            <Transactions<T>>::insert(tx_id, tx_data);
            <Corroborations<T>>::insert(tx_id, corroborations);
            Self::add_to_unresolved(tx_id)?;
        
            Ok(tx_id)
        }
        
        fn get_and_update_next_tx_id() -> u32 {
            let tx_id = NextTxId::<T>::get();
            NextTxId::<T>::put(tx_id + 1);
            tx_id
        }

        fn quorum_is_reached(entries: usize) -> bool {
            let quorum = calculate_one_third_quorum(AVN::<T>::validators().len() as u32);
            entries as u32 >= quorum
        }

        fn bound_params(
            params: Vec<(Vec<u8>, Vec<u8>)>,
        ) -> Result<BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>, Error<T>> {
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

        fn check_ethereum(tx_id: u32, expiry: u64) -> EthTxState {
            if let Ok(calldata) = Self::generate_corroboration_check_calldata(tx_id, expiry) {
                if let Ok(result) = Self::call_avn_bridge_contract_view_method(calldata) {
                    match result {
                        0 => return EthTxState::Unresolved,
                        1 => return EthTxState::Succeeded,
                        -1 => return EthTxState::Failed,
                        _ => {
                            log::error!(
                                "Invalid ethereum check response for tx_id {} and expiry {}: {}",
                                tx_id,
                                expiry,
                                result
                            );
                            return EthTxState::Unresolved
                        },
                    }
                }
            }

            log::error!("Invalid calldata generation for tx_id {} and expiry {}", tx_id, expiry);
            EthTxState::Unresolved
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
        
        // TODO: Make function private once the tests are configured to trigger the OCW
        pub fn generate_transaction_calldata(tx_id: u32) -> Result<Vec<u8>, Error<T>> {
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

        fn generate_corroboration_check_calldata(tx_id: u32, expiry: u64) -> Result<Vec<u8>, Error<T>> {
            let params = vec![
                (UINT32.to_vec(), tx_id.to_string().into_bytes()),
                (UINT256.to_vec(), expiry.to_string().into_bytes()),
            ];

            Self::encode_eth_function_input(&"corroborate".to_string(), &params)
        }

        fn encode_eth_function_input(
            function_name: &str,
            params: &[(Vec<u8>, Vec<u8>)],
        ) -> Result<Vec<u8>, Error<T>> {
            let inputs = params
                .iter()
                .filter_map(|(type_bytes, _)| {
                    Self::to_param_type(type_bytes).map(|kind| Param {
                        name: "".to_string(),
                        kind,
                    })
                })
                .collect::<Vec<_>>();
        
            let tokens: Result<Vec<_>, _> = params
                .iter()
                .map(|(type_bytes, value_bytes)| {
                    let param_type = Self::to_param_type(type_bytes).ok_or_else(|| Error::<T>::ParamTypeEncodingError)?;
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
        

        fn add_to_unresolved(tx_id: u32) -> Result<(), Error<T>> {
            UnresolvedTxList::<T>::try_mutate(|txs| -> Result<(), Error<T>> {
                txs.try_push(tx_id)
                    .map_err(|_| Error::<T>::UnresolvedTxLimitReached)
            })
        }

        fn remove_from_unresolved(tx_id: u32) -> DispatchResult {
            UnresolvedTxList::<T>::mutate(|txs| {
                if let Some(pos) = txs.iter().position(|&x| x == tx_id) {
                    txs.remove(pos);
                }
            });
            Ok(())
        }

        fn call_avn_bridge_contract_send_method(calldata: Vec<u8>) -> Result<H256, DispatchError> {
            Self::execute_avn_bridge_request(calldata, "send", Self::process_send_response)
        }

        fn call_avn_bridge_contract_view_method(
            calldata: Vec<u8>,
        ) -> Result<i8, DispatchError> {
            Self::execute_avn_bridge_request(calldata, "view", Self::process_view_response)
        }

        fn execute_avn_bridge_request<R>(
            calldata: Vec<u8>,
            endpoint: &str,
            process_response: fn(Vec<u8>) -> Result<R, DispatchError>,
        ) -> Result<R, DispatchError> {
            // TODO: replace with AVN pallet's get_bridge_contract_address() 
            let contract_address = H160(hex!("F05Df39f745A240fb133cC4a11E42467FAB10f1F"));
            // TODO: use actual self account
            let this_account: [u8; 32] = [0u8; 32];
            let transaction_to_send = EthTransaction::new(this_account, contract_address, calldata);

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

#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;

#[cfg(test)]
#[path = "tests/tests.rs"]
pub mod tests;
