//! # Aventus Node Pallet
//!
//! This pallet provides functionality that is common for other avn pallets such as handling
//! offchain worker validations, managing a list of validator accounts and their signing keys.
//!
//! The Authority defined here will also be shared by the other pallets that depend on AVN. This
//! means there will be 1 signing key for all AVN pallets (Ethereum-events,
//! Ethereum-transactions...). This key is separate from the rest of the session keys.
//!
//! From a security point of view, the rationale to implement it this way is because if one of the
//! signing keys is compromised, we dont want to trust that node for any other actions they carry
//! out so by having 1 key for our AVN pallets, we don't make our system less secure.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use frame_support::{dispatch::DispatchResult, log::*, traits::OneSessionHandler};
use pallet_session::{self as session};
use sp_application_crypto::RuntimeAppPublic;
use sp_avn_common::{
    event_types::Validator,
    offchain_worker_storage_lock::{self as OcwLock, OcwStorageError},
    recover_public_key_from_ecdsa_signature, DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER,
    EXTERNAL_SERVICE_PORT_NUMBER_KEY,
};
use sp_runtime::{
    offchain::{http, storage::StorageValueRef, Duration},
    traits::Member,
    DispatchError,
};
use sp_std::prelude::*;

use codec::{Decode, Encode};
use core::convert::TryInto;
pub use pallet::*;
use pallet_collator_selection as collator_selection;
use sp_core::ecdsa;

// Definition of the crypto to use for signing
pub mod sr25519 {
    mod app_sr25519 {
        use sp_application_crypto::{app_crypto, sr25519};
        app_crypto!(sr25519, sp_avn_common::AVN_KEY_ID);
    }

    // An identifier using sr25519 as its crypto.
    pub type AuthorityId = app_sr25519::Public;
}

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;

    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config + session::Config + collator_selection::Config {
        /// The identifier type for an authority.
        type AuthorityId: Member
            + Parameter
            + RuntimeAppPublic
            + Ord
            + MaybeSerializeDeserialize
            + MaxEncodedLen;
        type EthereumPublicKeyChecker: EthereumPublicKeyChecker<Self::AccountId>;
        /// A handler that will notify other pallets when a new session starts
        type NewSessionHandler: NewSessionHandler<Self::AuthorityId, Self::AccountId>;
        /// trait that allows the system to check for disabled validators
        type DisabledValidatorChecker: DisabledValidatorChecker<Self::AccountId>;
        /// trait that allows the runtime to check if a block is finalised
        type FinalisedBlockChecker: FinalisedBlockChecker<Self::BlockNumber>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::error]
    pub enum Error<T> {
        NotAValidator,
        NoLocalAccounts,
        OffchainWorkerAlreadyRun,
        ErrorConvertingAccountId,
        ErrorConvertingBlockNumber,
        ErrorConvertingUtf8,
        ErrorDecodingHex,
        ErrorRecordingOffchainWorkerRun,
        NoValidatorsFound,
        RequestTimedOut,
        DeadlineReached,
        UnexpectedStatusCode,
        InvalidVotingSession,
        DuplicateVote,
        InvalidVote,
        ErrorRecoveringPublicKeyFromSignature,
        InvalidECDSASignature,
    }

    #[pallet::storage]
    #[pallet::getter(fn validators)]
    /// The current set of validators (address and key) that may issue a transaction from the
    /// offchain worker.
    pub type Validators<T: Config> =
        StorageValue<_, Vec<Validator<T::AuthorityId, T::AccountId>>, ValueQuery>;
}

impl<T: Config> Pallet<T> {
    pub fn pre_run_setup(
        block_number: T::BlockNumber,
        caller_id: Vec<u8>,
    ) -> Result<Validator<T::AuthorityId, T::AccountId>, DispatchError> {
        if !sp_io::offchain::is_validator() {
            Err(Error::<T>::NotAValidator)?
        }

        let maybe_validator = Self::get_validator_for_current_node();
        if maybe_validator.is_none() {
            Err(Error::<T>::NoLocalAccounts)?
        }

        // Offchain workers could run multiple times for the same block number (re-orgs...)
        // so we need to make sure we only run this once per block
        OcwLock::record_block_run(block_number, caller_id).map_err(|e| match e {
            OcwStorageError::OffchainWorkerAlreadyRun => {
                info!("** Offchain worker has already run for block number: {:?}", block_number);
                Error::<T>::OffchainWorkerAlreadyRun
            },
            OcwStorageError::ErrorRecordingOffchainWorkerRun => {
                error!(
                    "** Unable to record offchain worker run for block {:?}, skipping",
                    block_number
                );
                Error::<T>::ErrorRecordingOffchainWorkerRun
            },
        })?;
        OcwLock::cleanup_expired_entries(&block_number);

        Ok(maybe_validator.expect("Already checked"))
    }

    // TODO [TYPE: refactoring][PRI: LOW]: choose a better function name
    pub fn is_primary(
        block_number: T::BlockNumber,
        current_validator: &T::AccountId,
    ) -> Result<bool, Error<T>> {
        let primary_validator = Self::calculate_primary_validator(block_number)?;
        return Ok(&primary_validator == current_validator)
    }

    pub fn calculate_primary_validator(
        block_number: T::BlockNumber,
    ) -> Result<T::AccountId, Error<T>> {
        let validators = Self::validators();

        // If there are no validators there's no point continuing
        if validators.len() == 0 {
            return Err(Error::<T>::NoValidatorsFound)
        }

        let block_number: usize = TryInto::<usize>::try_into(block_number)
            .map_err(|_| Error::<T>::ErrorConvertingBlockNumber)?;

        let index = block_number % validators.len();
        return Ok(validators[index].account_id.clone())
    }

    pub fn get_validator_for_current_node() -> Option<Validator<T::AuthorityId, T::AccountId>> {
        // This will return all keys whose keytype is set to `Ethereum_events`
        let mut local_keys = T::AuthorityId::all();
        let validators = Self::validators();
        local_keys.sort();

        return validators
            .into_iter()
            .enumerate()
            .filter_map(move |(_, validator)| {
                local_keys.binary_search(&validator.key).ok().map(|_| validator)
            })
            .nth(0)
    }

    pub fn get_data_from_service(url_path: String) -> Result<Vec<u8>, DispatchError> {
        let request = http::Request::default().method(http::Method::Get);
        return Ok(Self::invoke_external_service(request, url_path)?)
    }

    pub fn post_data_to_service(
        url_path: String,
        post_body: Vec<u8>,
    ) -> Result<Vec<u8>, DispatchError> {
        let request = http::Request::default().method(http::Method::Post).body(vec![post_body]);

        return Ok(Self::invoke_external_service(request, url_path)?)
    }

    pub fn request_ecdsa_signature_from_external_service(
        data_to_sign: &str,
    ) -> Result<ecdsa::Signature, DispatchError> {
        let mut url = String::from("eth/sign/");
        url.push_str(data_to_sign);

        let ecdsa_signature_utf8 = Self::get_data_from_service(url)?;
        let ecdsa_signature_bytes = core::str::from_utf8(&ecdsa_signature_utf8)
            .map_err(|_| Error::<T>::ErrorConvertingUtf8)?;

        let mut data: [u8; 65] = [0; 65];
        hex::decode_to_slice(ecdsa_signature_bytes, &mut data[0..65])
            .map_err(|_| Error::<T>::ErrorDecodingHex)?;
        Ok(ecdsa::Signature::from_raw(data))
    }

    pub fn signature_is_valid<D: Encode>(
        data: &D,
        validator: &Validator<T::AuthorityId, T::AccountId>,
        signature: &<T::AuthorityId as RuntimeAppPublic>::Signature,
    ) -> bool {
        // verify that the incoming (unverified) pubkey is actually a validator
        if !Self::is_validator(&validator.account_id) {
            return false
        }

        // check signature (this is expensive so we do it last).
        let signature_valid =
            data.using_encoded(|encoded_data| validator.key.verify(&encoded_data, &signature));

        return signature_valid
    }

    pub fn eth_signature_is_valid(
        data: String,
        validator: &Validator<T::AuthorityId, T::AccountId>,
        signature: &ecdsa::Signature,
    ) -> bool {
        // verify that the incoming (unverified) pubkey is actually a validator
        if !Self::is_validator(&validator.account_id) {
            return false
        }

        let recovered_public_key = recover_public_key_from_ecdsa_signature(signature.clone(), data);
        if recovered_public_key.is_err() {
            return false
        }

        match T::EthereumPublicKeyChecker::get_validator_for_eth_public_key(
            &recovered_public_key.expect("Checked for error"),
        ) {
            Some(maybe_validator) => maybe_validator == validator.account_id,
            _ => false,
        }
    }

    pub fn convert_block_number_to_u32(block_number: T::BlockNumber) -> Result<u32, Error<T>> {
        let block_number: u32 = TryInto::<u32>::try_into(block_number)
            .map_err(|_| Error::<T>::ErrorConvertingBlockNumber)?;

        Ok(block_number)
    }

    pub fn is_validator(account_id: &T::AccountId) -> bool {
        return Self::validators().into_iter().any(|v| v.account_id == *account_id)
    }

    pub fn active_validators() -> Vec<Validator<T::AuthorityId, T::AccountId>> {
        return Self::validators()
    }

    pub fn try_get_validator(
        account_id: &T::AccountId,
    ) -> Option<Validator<T::AuthorityId, T::AccountId>> {
        return Self::validators().into_iter().filter(|v| v.account_id == *account_id).nth(0)
    }

    /// This function will mutate storage. Any code after calling this MUST not error.
    pub fn remove_validator_from_active_list(validator_id: &T::AccountId) {
        <Validators<T>>::mutate(|active_validators| {
            if let Some(validator_index) =
                active_validators.iter().position(|v| &v.account_id == validator_id)
            {
                active_validators.remove(validator_index);
            }
        });
    }

    pub fn calculate_two_third_quorum() -> u32 {
        let len = Self::active_validators().len() as u32;
        if len < 3 {
            return len
        } else {
            return (2 * len / 3) + 1
        }
    }

    pub fn is_block_finalised(block_number: T::BlockNumber) -> bool {
        return T::FinalisedBlockChecker::is_finalised(block_number)
    }

    pub fn get_external_service_port_number() -> String {
        let stored_value =
            StorageValueRef::persistent(EXTERNAL_SERVICE_PORT_NUMBER_KEY).get::<Vec<u8>>();
        let port_number_bytes = match stored_value {
            Ok(Some(port)) => port,
            _ => DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER.into(),
        };

        let port_number = core::str::from_utf8(&port_number_bytes);
        if let Err(e) = port_number {
            trace!(
                "❌ External service port {} is not formatted correctly. Using default port.",
                e
            );
            return DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER.into()
        }

        let port_number = port_number.expect("Already checked for errors");

        if port_number.parse::<u32>().is_err() {
            trace!(
                "❌ External service port {} is not a valid number. Using default port.",
                port_number
            );
            return DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER.into()
        }

        return port_number.into()
    }

    fn invoke_external_service(
        request: http::Request<Vec<Vec<u8>>>,
        url_path: String,
    ) -> Result<Vec<u8>, DispatchError> {
        // TODO: Make this configurable
        let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(300_000));
        let external_service_port_number = Self::get_external_service_port_number();

        let mut url = String::from("http://127.0.0.1:");
        url.push_str(&external_service_port_number);
        url.push_str(&"/".to_string());
        url.push_str(&url_path);

        let pending = request
            .deadline(deadline)
            .url(&url)
            .send()
            .map_err(|_| Error::<T>::RequestTimedOut)?;

        let response = pending
            .try_wait(deadline)
            .map_err(|_| Error::<T>::DeadlineReached)?
            .map_err(|_| Error::<T>::DeadlineReached)?;

        if response.code != 200 {
            error!("❌ Unexpected status code: {}", response.code);
            return Err(Error::<T>::UnexpectedStatusCode)?
        }

        let result: Vec<u8> = response.body().collect::<Vec<u8>>();
        return Ok(result)
    }
}

// Session pallet interface

impl<T: Config> sp_runtime::BoundToRuntimeAppPublic for Pallet<T> {
    type Public = T::AuthorityId;
}

impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
    type Key = T::AuthorityId;

    fn on_genesis_session<'a, I: 'a>(validators: I)
    where
        I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
    {
        trace!("Avn pallet genesis session entrypoint");
        let avn_validators =
            validators.map(|x| Validator::new(x.0.clone(), x.1)).collect::<Vec<_>>();
        if !avn_validators.is_empty() {
            assert!(Validators::<T>::get().is_empty(), "Validators are already initialized!");
            Validators::<T>::put(&avn_validators);

            T::NewSessionHandler::on_genesis_session(&avn_validators);
        }
    }

    fn on_new_session<'a, I: 'a>(changed: bool, validators: I, _queued_validators: I)
    where
        I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
    {
        trace!("Avn pallet new session entrypoint");
        // Update the list of validators if it has changed
        let mut disabled_avn_validators: Vec<T::AccountId> = vec![];
        let mut active_avn_validators: Vec<Validator<T::AuthorityId, T::AccountId>> = vec![];

        validators.for_each(|x| {
            if T::DisabledValidatorChecker::is_disabled(x.0) {
                disabled_avn_validators.push(x.0.clone());
            } else {
                active_avn_validators.push(Validator::new(x.0.clone(), x.1));
            }
        });

        if changed {
            Validators::<T>::put(&active_avn_validators);
        }

        T::NewSessionHandler::on_new_session(
            changed,
            &active_avn_validators,
            &disabled_avn_validators,
        );
    }

    fn on_disabled(_i: u32) {
        // ignore
    }
}

/// Provides the new set of validator_account_ids to the session module when a session is being
/// rotated (ended).
impl<T: Config> session::SessionManager<T::AccountId> for Pallet<T> {
    fn new_session(new_index: u32) -> Option<Vec<T::AccountId>> {
        let collators = <collator_selection::Pallet<T>>::new_session(new_index)
            .or_else(|| Some(Vec::<T::AccountId>::new()))
            .expect("We always have an iterable");

        debug!(
            "[AVN] assembling new collators for new session {} with these validators {:#?} at #{:?}",
            new_index,
            collators,
            <frame_system::Pallet<T>>::block_number(),
        );

        return Some(collators)
    }

    fn end_session(end_index: u32) {
        <collator_selection::Pallet<T>>::end_session(end_index);
    }

    fn start_session(start_index: u32) {
        debug!(
            "[validators-manager] starting new session {} at #{:?}",
            start_index,
            <frame_system::Pallet<T>>::block_number(),
        );

        <collator_selection::Pallet<T>>::start_session(start_index);
    }
}

pub trait AccountToBytesConverter<AccountId: Decode + Encode> {
    fn into_bytes(account: &AccountId) -> [u8; 32];
    fn try_from(account_bytes: &[u8; 32]) -> Result<AccountId, DispatchError>;
    /// This function expects valid bytes that can be converted into an accountId. No validation is
    /// done here.
    fn try_from_any(bytes: Vec<u8>) -> Result<AccountId, DispatchError>;
}

impl<T: Config> AccountToBytesConverter<T::AccountId> for Pallet<T> {
    fn into_bytes(account: &T::AccountId) -> [u8; 32] {
        let bytes = account.encode();
        let mut vector: [u8; 32] = Default::default();
        vector.copy_from_slice(&bytes[0..32]);
        return vector
    }

    fn try_from(account_bytes: &[u8; 32]) -> Result<T::AccountId, DispatchError> {
        let account_result = T::AccountId::decode(&mut &account_bytes[..]);
        account_result.map_err(|_| DispatchError::Other("Error converting AccountId"))
    }

    fn try_from_any(bytes: Vec<u8>) -> Result<T::AccountId, DispatchError> {
        let mut account_bytes: [u8; 32] = Default::default();
        account_bytes.copy_from_slice(&bytes[0..32]);

        return T::AccountId::decode(&mut &account_bytes[..])
            .map_err(|_| DispatchError::Other("Error converting to AccountId"))
    }
}

pub trait EthereumPublicKeyChecker<AccountId: Member> {
    fn get_validator_for_eth_public_key(eth_public_key: &ecdsa::Public) -> Option<AccountId>;
}

impl<AccountId: Member> EthereumPublicKeyChecker<AccountId> for () {
    fn get_validator_for_eth_public_key(_eth_public_key: &ecdsa::Public) -> Option<AccountId> {
        None
    }
}

pub trait NewSessionHandler<AuthorityId: Member, AccountId: Member> {
    fn on_genesis_session(validators: &Vec<Validator<AuthorityId, AccountId>>);
    fn on_new_session(
        changed: bool,
        active_validators: &Vec<Validator<AuthorityId, AccountId>>,
        disabled_validators: &Vec<AccountId>,
    );
}

pub trait DisabledValidatorChecker<AccountId: Member> {
    fn is_disabled(validator_account_id: &AccountId) -> bool;
}

impl<AccountId: Member> DisabledValidatorChecker<AccountId> for () {
    fn is_disabled(_validator_account_id: &AccountId) -> bool {
        false
    }
}

pub trait ValidatorRegistrationNotifier<ValidatorId: Member> {
    fn on_validator_registration(validator_id: &ValidatorId);
}

impl<ValidatorId: Member> ValidatorRegistrationNotifier<ValidatorId> for () {
    fn on_validator_registration(_validator_id: &ValidatorId) {}
}

pub trait Enforcer<ValidatorId: Member> {
    fn slash_validator(slashed_validator_id: &ValidatorId) -> DispatchResult;
}

impl<ValidatorId: Member> Enforcer<ValidatorId> for () {
    fn slash_validator(_slashed_validator_id: &ValidatorId) -> DispatchResult {
        Ok(())
    }
}

pub trait FinalisedBlockChecker<BlockNumber: Member> {
    fn is_finalised(block_number: BlockNumber) -> bool;
}

impl<BlockNumber: Member> FinalisedBlockChecker<BlockNumber> for () {
    fn is_finalised(_block_number: BlockNumber) -> bool {
        false
    }
}

pub trait OnGrowthLiftedHandler<Balance> {
    fn on_growth_lifted(amount: Balance, growth_period: u32) -> DispatchResult;
}

impl<Balance> OnGrowthLiftedHandler<Balance> for () {
    fn on_growth_lifted(_amount: Balance, _growth_period: u32) -> DispatchResult {
        Ok(())
    }
}
