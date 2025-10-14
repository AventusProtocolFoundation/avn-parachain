//! # Validators manager Pallet
//!
//! This pallet provides functionality to add/remove validators.
//!
//! The pallet is based on the Substrate session pallet and implements related traits for session
//! management when validators are added or removed.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use sp_avn_common::eth::EthereumId;

use frame_support::{
    dispatch::DispatchResult,
    ensure,
    pallet_prelude::StorageVersion,
    traits::{Currency, Get},
    transactional,
};
use frame_system::{offchain::SendTransactionTypes, pallet_prelude::BlockNumberFor, RawOrigin};
use pallet_session::{self as session, Config as SessionConfig};
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{Convert, Member},
    DispatchError,
};
use sp_std::prelude::*;

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_avn::{
    self as avn, AccountToBytesConverter, BridgeInterfaceNotification, DisabledValidatorChecker,
    Enforcer, EthereumPublicKeyChecker, NewSessionHandler, ProcessedEventsChecker,
    ValidatorRegistrationNotifier, MAX_VALIDATOR_ACCOUNTS,
};

use sp_avn_common::{
    bounds::MaximumValidatorsBound,
    eth_key_actions::decompress_eth_public_key,
    event_types::Validator,
    BridgeContractMethod,
    IngressCounter,
};

#[cfg(any(test, feature = "runtime-benchmarks"))]
use sp_avn_common::eth_key_actions::compress_eth_public_key;
use sp_core::{bounded::BoundedVec, ecdsa};

pub use pallet_parachain_staking::{self as parachain_staking, BalanceOf, PositiveImbalanceOf};

use pallet_avn::BridgeInterface;

pub use pallet::*;

const PALLET_ID: &'static [u8; 14] = b"author_manager";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{assert_ok, pallet_prelude::*};
    use frame_system::pallet_prelude::*;

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config:
        SendTransactionTypes<Call<Self>>
        + frame_system::Config
        + session::Config
        + avn::Config
        + parachain_staking::Config
        + pallet_session::historical::Config
    {
        /// Overarching event type
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as frame_system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// A trait that allows to subscribe to notifications triggered when ethereum event
        /// processes an event
        type ProcessedEventsChecker: ProcessedEventsChecker;
        /// A period (in block number) where validators are allowed to vote
        type VotingPeriod: Get<BlockNumberFor<Self>>;
        /// A trait that allows converting between accountIds <-> public keys
        type AccountToBytesConvert: AccountToBytesConverter<Self::AccountId>;
        /// A trait that allows extra work to be done during validator registration
        type ValidatorRegistrationNotifier: ValidatorRegistrationNotifier<
            <Self as session::Config>::ValidatorId,
        >;

        /// Weight information for the extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        type BridgeInterface: avn::BridgeInterface;
        /// Minimum number of validators that must remain active
        #[pallet::constant]
        type MinimumValidatorCount: Get<u32>;
    }

    #[pallet::error]
    pub enum Error<T> {
        NoTier1EventForAddingValidator,
        NoTier1EventForRemovingValidator,
        NoValidators,
        ValidatorAlreadyExists,
        InvalidIngressCounter,
        MinimumValidatorsReached,
        ErrorEndingVotingPeriod,
        VotingSessionIsNotValid,
        ErrorSubmitCandidateTxnToTier1,
        ErrorCalculatingPrimaryValidator,
        ErrorGeneratingEthDescription,
        ValidatorsActionDataNotFound,
        RemovalAlreadyRequested,
        ErrorConvertingAccountIdToValidatorId,
        SlashedValidatorIsNotFound,
        ValidatorNotFound,
        InvalidPublicKey,
        /// The ethereum public key of this validator alredy exists
        ValidatorEthKeyAlreadyExists,
        ErrorRemovingAccountFromCollators,
        MaximumValidatorsReached,
        /// Account is already a candidate
        AlreadyCandidate,
        /// Deposit is below minimum required stake
        DepositBelowMinimum,
        /// Account has insufficient balance for deposit
        InsufficientBalance,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ValidatorRegistered {
            validator_id: T::AccountId,
            eth_key: ecdsa::Public,
        },
        ValidatorDeregistered {
            validator_id: T::AccountId,
        },
        ValidatorActivationStarted {
            validator_id: T::AccountId,
        },
        ValidatorActionConfirmed {
            action_id: ActionId<T::AccountId>,
        },
        ValidatorSlashed {
            action_id: ActionId<T::AccountId>,
        },
        PublishingValidatorActionOnEthereumFailed {
            tx_id: u32,
        },
        PublishingValidatorActionOnEthereumSucceeded {
            tx_id: u32,
        },
    }

    #[pallet::storage]
    #[pallet::getter(fn validator_account_ids)]
    pub type ValidatorAccountIds<T: Config> =
        StorageValue<_, BoundedVec<T::AccountId, MaximumValidatorsBound>>;

    #[pallet::storage]
    pub type ValidatorActions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        IngressCounter,
        ValidatorsActionData,
        OptionQuery,
        GetDefault,
    >;

    #[pallet::storage]
    #[pallet::getter(fn get_validator_by_eth_public_key)]
    pub type EthereumPublicKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, ecdsa::Public, T::AccountId>;

    #[pallet::storage]
    #[pallet::getter(fn get_ingress_counter)]
    pub type TotalIngresses<T: Config> = StorageValue<_, IngressCounter, ValueQuery>;

    /// Stores deposit amounts for pending registrations 
    #[pallet::storage]
    pub type PendingRegistrationDeposits<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub validators: Vec<(T::AccountId, ecdsa::Public)>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { validators: Vec::<(T::AccountId, ecdsa::Public)>::new() }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            log::debug!(
                "Validators Manager Genesis build entrypoint - total validators: {}",
                self.validators.len()
            );
            for (validator_account_id, eth_public_key) in &self.validators {
                assert_ok!(<ValidatorAccountIds<T>>::try_append(validator_account_id));
                <EthereumPublicKeys<T>>::insert(eth_public_key, validator_account_id);
            }

            // Set storage version
            STORAGE_VERSION.put::<Pallet<T>>();
            log::debug!(
                "Validators manager storage chain/current storage version: {:?} / {:?}",
                Pallet::<T>::on_chain_storage_version(),
                Pallet::<T>::current_storage_version(),
            );
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Sudo function to add a collator.
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::add_collator())]
        #[transactional]
        pub fn add_collator(
            origin: OriginFor<T>,
            collator_account_id: T::AccountId,
            collator_eth_public_key: ecdsa::Public,
            deposit: Option<BalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;

            // Validate the registration request
            Self::validate_validator_registration_request(
                &collator_account_id,
                &collator_eth_public_key,
            )?;

            let bond =
                deposit.unwrap_or_else(|| parachain_staking::Pallet::<T>::min_collator_stake());

            Self::validate_staking_preconditions(&collator_account_id, bond)?;

            // Store deposit for use in callback
            PendingRegistrationDeposits::<T>::insert(&collator_account_id, bond);

            // Send to T1 - actual registration happens in callback
            Self::send_validator_registration_to_t1(
                &collator_account_id,
                &collator_eth_public_key,
            )?;

            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_validator(MAX_VALIDATOR_ACCOUNTS))]
        #[transactional]
        pub fn remove_validator(
            origin: OriginFor<T>,
            collator_account_id: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            let _ = ensure_root(origin)?;

            // Validate the deregistration request
            Self::validate_validator_deregistration_request(&collator_account_id)?;

            // Send to T1 - actual deregistration happens in callback
            Self::send_validator_deregistration_to_t1(&collator_account_id)?;

            Ok(().into())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::rotate_validator_ethereum_key())]
        #[transactional]
        pub fn rotate_validator_ethereum_key(
            origin: OriginFor<T>,
            author_account_id: T::AccountId,
            author_old_eth_public_key: ecdsa::Public,
            author_new_eth_public_key: ecdsa::Public,
        ) -> DispatchResult {
            let _ = ensure_root(origin)?;

            ensure!(
                !<EthereumPublicKeys<T>>::contains_key(&author_new_eth_public_key),
                Error::<T>::ValidatorEthKeyAlreadyExists
            );
            ensure!(
                author_old_eth_public_key != author_new_eth_public_key,
                Error::<T>::ValidatorEthKeyAlreadyExists
            );

            let author_id = EthereumPublicKeys::<T>::take(&author_old_eth_public_key)
                .ok_or(Error::<T>::ValidatorNotFound)?;
            ensure!(author_id == author_account_id, Error::<T>::ValidatorNotFound);

            EthereumPublicKeys::<T>::insert(author_new_eth_public_key, author_id);
            Ok(())
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
pub enum ValidatorsActionStatus {
    /// Validator enters this state immediately within removal extrinsic, ready for session
    /// confirmation
    AwaitingConfirmation,
    /// Validator enters this state within session handler, ready for signing and sending to T1
    Confirmed,
    /// Validator enters this state once T1 action request is sent, ready to be removed from
    /// hashmap
    Actioned,
    /// Validator enters this state once T1 event processed
    None,
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
pub enum ValidatorsActionType {
    /// Validator has asked to leave voluntarily
    Resignation,
    /// Validator is being forced to leave due to a malicious behaviour
    Slashed,
    /// Validator activates himself after he joins an active session
    Activation,
    /// Default value
    Unknown,
    /// Validator registration pending T1 confirmation
    Registration,
}

impl ValidatorsActionType {
    fn is_deregistration(&self) -> bool {
        match self {
            ValidatorsActionType::Resignation => true,
            ValidatorsActionType::Slashed => true,
            _ => false,
        }
    }

    fn is_activation(&self) -> bool {
        match self {
            ValidatorsActionType::Activation => true,
            _ => false,
        }
    }

    fn is_registration(&self) -> bool {
        match self {
            ValidatorsActionType::Registration => true,
            _ => false,
        }
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ValidatorsActionData {
    pub status: ValidatorsActionStatus,
    pub eth_transaction_id: EthereumId,
    pub action_type: ValidatorsActionType,
}

impl ValidatorsActionData {
    fn new(
        status: ValidatorsActionStatus,
        eth_transaction_id: EthereumId,
        action_type: ValidatorsActionType,
    ) -> Self {
        ValidatorsActionData { status, eth_transaction_id, action_type }
    }
}

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;

mod benchmarking;

pub mod default_weights;
pub use default_weights::WeightInfo;

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

pub type AVN<T> = avn::Pallet<T>;

impl<T: Config> Pallet<T> {
    /// Helper function to compress eth public key (exposed for testing and benchmarking)
    #[cfg(any(test, feature = "runtime-benchmarks"))]
    pub fn compress_eth_public_key(full_public_key: sp_core::H512) -> ecdsa::Public {
        compress_eth_public_key(full_public_key)
    }

    fn remove_ethereum_public_key_if_required(validator_id: &T::AccountId) {
        if let Some(public_key_to_remove) = Self::get_ethereum_public_key_if_exists(validator_id) {
            <EthereumPublicKeys<T>>::remove(public_key_to_remove);
        }
    }

    fn get_ethereum_public_key_if_exists(account_id: &T::AccountId) -> Option<ecdsa::Public> {
        <EthereumPublicKeys<T>>::iter()
            .find(|(_, acc)| acc == account_id)
            .map(|(pk, _)| pk)
    }

    fn validator_permanently_removed(
        active_validators: &Vec<Validator<T::AuthorityId, T::AccountId>>,
        disabled_validators: &Vec<T::AccountId>,
        deregistered_validator: &T::AccountId,
    ) -> bool {
        !active_validators.iter().any(|v| &v.account_id == deregistered_validator) &&
            !disabled_validators.iter().any(|v| v == deregistered_validator)
    }

    fn deregistration_state_is_active(status: ValidatorsActionStatus) -> bool {
        matches!(
            status,
            ValidatorsActionStatus::AwaitingConfirmation | ValidatorsActionStatus::Confirmed
        )
    }

    fn has_active_slash(validator_account_id: &T::AccountId) -> bool {
        <ValidatorActions<T>>::iter_prefix_values(validator_account_id).any(
            |validators_action_data| {
                validators_action_data.action_type == ValidatorsActionType::Slashed &&
                    Self::deregistration_state_is_active(validators_action_data.status)
            },
        )
    }

    fn validate_validator_registration_request(
        account_id: &T::AccountId,
        eth_public_key: &ecdsa::Public,
    ) -> DispatchResult {
        let validator_account_ids =
            Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;
        ensure!(!validator_account_ids.is_empty(), Error::<T>::NoValidators);

        ensure!(!validator_account_ids.contains(account_id), Error::<T>::ValidatorAlreadyExists);

        ensure!(
            !<EthereumPublicKeys<T>>::contains_key(eth_public_key),
            Error::<T>::ValidatorEthKeyAlreadyExists
        );

        ensure!(
            validator_account_ids.len() <
                (<MaximumValidatorsBound as sp_core::TypedGet>::get() as usize),
            Error::<T>::MaximumValidatorsReached
        );

        Ok(())
    }

    fn validate_validator_deregistration_request(account_id: &T::AccountId) -> DispatchResult {
        let validator_account_ids =
            Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;

        ensure!(
            validator_account_ids.len() > T::MinimumValidatorCount::get() as usize,
            Error::<T>::MinimumValidatorsReached
        );

        ensure!(validator_account_ids.contains(account_id), Error::<T>::ValidatorNotFound);

        // Check for conflicting deregistration already in progress
        ensure!(
            !Self::has_active_deregistration(account_id),
            Error::<T>::RemovalAlreadyRequested
        );

        Ok(())
    }

    fn has_active_deregistration(validator_account_id: &T::AccountId) -> bool {
        <ValidatorActions<T>>::iter_prefix_values(validator_account_id).any(
            |validators_action_data| {
                validators_action_data.action_type.is_deregistration() &&
                    Self::deregistration_state_is_active(validators_action_data.status)
            },
        )
    }

    fn validate_staking_preconditions(
        account_id: &T::AccountId,
        deposit: BalanceOf<T>,
    ) -> DispatchResult {
        // Check 1: Not already a candidate
        ensure!(
            parachain_staking::Pallet::<T>::candidate_info(account_id).is_none(),
            Error::<T>::AlreadyCandidate
        );

        // Check 2: Deposit meets minimum
        let min_stake = parachain_staking::Pallet::<T>::min_collator_stake();
        ensure!(deposit >= min_stake, Error::<T>::DepositBelowMinimum);

        // Check 3: Account has sufficient free balance
        type CurrencyOf<T> = <T as parachain_staking::Config>::Currency;
        let free_balance = CurrencyOf::<T>::free_balance(account_id);

        ensure!(free_balance >= deposit, Error::<T>::InsufficientBalance);

        Ok(())
    }

    fn clean_up_staking_data(action_account_id: T::AccountId) -> Result<(), ()> {
        // Cleanup staking state for the collator we are removing
        let staking_state = parachain_staking::Pallet::<T>::candidate_info(&action_account_id);
        if staking_state.is_none() {
            log::error!(
                "Unable to find staking candidate info for collator: {:?}",
                action_account_id
            );
            return Err(())
        }

        let staking_state = staking_state.expect("Checked for none already");

        let result = parachain_staking::Pallet::<T>::execute_leave_candidates(
            <T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(
                action_account_id.clone(),
            )),
            action_account_id.clone(),
            staking_state.nomination_count,
        );

        if result.is_err() {
            log::error!(
                "Error removing staking data for collator {:?}: {:?}",
                action_account_id,
                result
            );
            return Err(())
        }

        Ok(())
    }

    fn clean_up_collator_data(action_account_id: T::AccountId, ingress_counter: IngressCounter) {
        if let Ok(()) = Self::clean_up_staking_data(action_account_id.clone()) {
            <ValidatorActions<T>>::mutate(
                &action_account_id,
                ingress_counter,
                |validators_action_data_maybe| {
                    if let Some(validators_action_data) = validators_action_data_maybe {
                        validators_action_data.status = ValidatorsActionStatus::Confirmed
                    }
                },
            );
            Self::remove_ethereum_public_key_if_required(&action_account_id);

            let action_id = ActionId::new(action_account_id, ingress_counter);

            Self::deposit_event(Event::<T>::ValidatorActionConfirmed { action_id });
        }
    }


    /// Send validator registration request to T1
    fn send_validator_registration_to_t1(
        validator_account_id: &T::AccountId,
        validator_eth_public_key: &ecdsa::Public,
    ) -> Result<EthereumId, DispatchError> {
        // Add eth key mapping immediately (before T1 confirmation)
        <EthereumPublicKeys<T>>::insert(validator_eth_public_key, validator_account_id);

        // Prepare data for T1
        let decompressed_eth_public_key = decompress_eth_public_key(*validator_eth_public_key)
            .map_err(|_| Error::<T>::InvalidPublicKey)?;

        let validator_id_bytes =
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(validator_account_id);

        let function_name = BridgeContractMethod::AddAuthor.name_as_bytes();
        let params = vec![
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
            (b"bytes32".to_vec(), validator_id_bytes.to_vec()),
        ];

        let tx_id = <T as pallet::Config>::BridgeInterface::publish(
            function_name,
            &params,
            PALLET_ID.to_vec(),
        )
        .map_err(|_| Error::<T>::ErrorSubmitCandidateTxnToTier1)?;

        // Now create ValidatorActions entry with the actual tx_id (single insert, no mutation)
        let ingress_counter = Self::get_ingress_counter() + 1;
        TotalIngresses::<T>::put(ingress_counter);

        <ValidatorActions<T>>::insert(
            validator_account_id,
            ingress_counter,
            ValidatorsActionData::new(
                ValidatorsActionStatus::AwaitingConfirmation,
                tx_id,
                ValidatorsActionType::Registration,
            ),
        );

        Self::deposit_event(Event::<T>::PublishingValidatorActionOnEthereumSucceeded { tx_id });

        Ok(tx_id)
    }

    /// Send validator deregistration request to T1
    fn send_validator_deregistration_to_t1(
        validator_account_id: &T::AccountId,
    ) -> Result<EthereumId, DispatchError> {
        // Prepare data for T1
        let eth_public_key = Self::get_ethereum_public_key_if_exists(validator_account_id)
            .ok_or(Error::<T>::ValidatorNotFound)?;

        let decompressed_eth_public_key =
            decompress_eth_public_key(eth_public_key).map_err(|_| Error::<T>::InvalidPublicKey)?;

        let validator_id_bytes =
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(validator_account_id);

        let function_name = BridgeContractMethod::RemoveAuthor.name_as_bytes();
        let params = vec![
            (b"bytes32".to_vec(), validator_id_bytes.to_vec()),
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
        ];

        // Send to T1 and get tx_id FIRST
        let tx_id = <T as pallet::Config>::BridgeInterface::publish(
            function_name,
            &params,
            PALLET_ID.to_vec(),
        )
        .map_err(|_| Error::<T>::ErrorSubmitCandidateTxnToTier1)?;

        // Now create ValidatorActions entry with the actual tx_id (single insert, no mutation)
        let ingress_counter = Self::get_ingress_counter() + 1;
        TotalIngresses::<T>::put(ingress_counter);

        <ValidatorActions<T>>::insert(
            validator_account_id,
            ingress_counter,
            ValidatorsActionData::new(
                ValidatorsActionStatus::AwaitingConfirmation,
                tx_id,
                ValidatorsActionType::Resignation,
            ),
        );

        Self::deposit_event(Event::<T>::PublishingValidatorActionOnEthereumSucceeded { tx_id });

        Ok(tx_id)
    }
}

impl<T: Config> BridgeInterfaceNotification for Pallet<T> {
    fn process_result(tx_id: u32, caller_id: Vec<u8>, succeeded: bool) -> DispatchResult {
        if caller_id != PALLET_ID.to_vec() {
            return Ok(())
        }

        // Find the ValidatorActions entry with matching tx_id
        let mut found_entry: Option<(T::AccountId, IngressCounter, ValidatorsActionType)> = None;
        
        for (account_id, ingress_counter, validators_action_data) in <ValidatorActions<T>>::iter() {
            if validators_action_data.eth_transaction_id == tx_id {
                found_entry = Some((
                    account_id,
                    ingress_counter,
                    validators_action_data.action_type,
                ));
                break;
            }
        }

        let Some((account_id, ingress_counter, action_type)) = found_entry else {
            // No matching entry found - might have been cleaned up already
            return Ok(())
        };

        if !succeeded {
            // T1 operation failed - cleanup the action and rollback state
            if action_type.is_registration() {
                // Remove the eth key mapping we added optimistically
                if let Some(eth_key) = Self::get_ethereum_public_key_if_exists(&account_id) {
                    <EthereumPublicKeys<T>>::remove(eth_key);
                }
                // Cleanup stored deposit
                PendingRegistrationDeposits::<T>::remove(&account_id);
            }
            
            <ValidatorActions<T>>::remove(&account_id, ingress_counter);
            Self::deposit_event(Event::<T>::PublishingValidatorActionOnEthereumFailed { tx_id });
            return Ok(())
        }

        // T1 succeeded - complete the operation based on action type
        if action_type.is_registration() {
            // Complete registration
            let deposit = PendingRegistrationDeposits::<T>::take(&account_id)
                .unwrap_or_else(|| parachain_staking::Pallet::<T>::min_collator_stake());
            
            let candidate_count = parachain_staking::Pallet::<T>::candidate_pool().0.len() as u32;
            
            match parachain_staking::Pallet::<T>::join_candidates(
                <T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(
                    account_id.clone(),
                )),
                deposit,
                candidate_count,
            ) {
                Ok(_) => {},
                Err(e) => {
                    log::error!(
                        "Failed to join candidates for {:?}: {:?}",
                        account_id,
                        e
                    );
                    // Cleanup on failure
                    if let Some(eth_key) = Self::get_ethereum_public_key_if_exists(&account_id) {
                        <EthereumPublicKeys<T>>::remove(eth_key);
                    }
                    <ValidatorActions<T>>::remove(&account_id, ingress_counter);
                    return Err(e.error)
                },
            }

            // Add to active validators list
            match <ValidatorAccountIds<T>>::try_append(account_id.clone()) {
                Ok(_) => {},
                Err(_) => {
                    log::error!("Failed to append validator to ValidatorAccountIds");
                    // Cleanup on failure
                    if let Some(eth_key) = Self::get_ethereum_public_key_if_exists(&account_id) {
                        <EthereumPublicKeys<T>>::remove(eth_key);
                    }
                    <ValidatorActions<T>>::remove(&account_id, ingress_counter);
                    return Err(Error::<T>::MaximumValidatorsReached.into())
                }
            }

            // Notify validator registration
            let new_validator_id =
                <T as SessionConfig>::ValidatorIdOf::convert(account_id.clone())
                    .ok_or(Error::<T>::ErrorConvertingAccountIdToValidatorId)?;
            T::ValidatorRegistrationNotifier::on_validator_registration(&new_validator_id);

            // Get eth key for event (we know it exists because we added it earlier)
            let eth_key = Self::get_ethereum_public_key_if_exists(&account_id)
                .ok_or(Error::<T>::ValidatorNotFound)?;

            // Update ValidatorActions for activation process
            <ValidatorActions<T>>::mutate(
                &account_id,
                ingress_counter,
                |validators_action_data_maybe| {
                    if let Some(validators_action_data) = validators_action_data_maybe {
                        validators_action_data.action_type = ValidatorsActionType::Activation;
                    }
                },
            );

            // Emit success event
            Self::deposit_event(Event::<T>::ValidatorRegistered {
                validator_id: account_id,
                eth_key,
            });
        } else if action_type.is_deregistration() {
            // For deregistration, initiate the staking exit
            // This will cause ParachainStaking to remove them from next session
            let candidate_count = parachain_staking::Pallet::<T>::candidate_pool().0.len() as u32;
            match parachain_staking::Pallet::<T>::schedule_leave_candidates(
                <T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(
                    account_id.clone(),
                )),
                candidate_count,
            ) {
                Ok(_) => {},
                Err(e) => {
                    log::error!(
                        "Failed to schedule leave candidates for {:?}: {:?}",
                        account_id,
                        e
                    );
                    // Leave ValidatorActions entry so session handler can retry
                    return Err(e.error)
                },
            }
            // Session handler will complete the cleanup via clean_up_collator_data
        }

        Ok(())
    }
}

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Debug, Eq, TypeInfo, MaxEncodedLen)]
pub struct ActionId<AccountId: Member> {
    pub action_account_id: AccountId,
    pub ingress_counter: IngressCounter,
}

impl<AccountId: Member + Encode> ActionId<AccountId> {
    fn new(action_account_id: AccountId, ingress_counter: IngressCounter) -> Self {
        ActionId::<AccountId> { action_account_id, ingress_counter }
    }
}

impl<T: Config> NewSessionHandler<T::AuthorityId, T::AccountId> for Pallet<T> {
    fn on_genesis_session(_validators: &Vec<Validator<T::AuthorityId, T::AccountId>>) {
        log::trace!("Validators manager on_genesis_session");
    }

    fn on_new_session(
        _changed: bool,
        active_validators: &Vec<Validator<T::AuthorityId, T::AccountId>>,
        disabled_validators: &Vec<T::AccountId>,
    ) {
        log::trace!("Validators manager on_new_session");
        if <ValidatorActions<T>>::iter().count() > 0 {
            for (action_account_id, ingress_counter, validators_action_data) in
                <ValidatorActions<T>>::iter()
            {
                if validators_action_data.status == ValidatorsActionStatus::AwaitingConfirmation &&
                    validators_action_data.action_type.is_deregistration() &&
                    Self::validator_permanently_removed(
                        &active_validators,
                        &disabled_validators,
                        &action_account_id,
                    )
                {
                    Self::clean_up_collator_data(action_account_id, ingress_counter);
                } else if validators_action_data.status ==
                    ValidatorsActionStatus::AwaitingConfirmation &&
                    validators_action_data.action_type.is_activation()
                {
                    <ValidatorActions<T>>::mutate(
                        &action_account_id,
                        ingress_counter,
                        |validators_action_data_maybe| {
                            if let Some(validators_action_data) = validators_action_data_maybe {
                                validators_action_data.status = ValidatorsActionStatus::Confirmed
                            }
                        },
                    );

                    Self::deposit_event(Event::<T>::ValidatorActivationStarted {
                        validator_id: action_account_id.clone(),
                    });
                } else if validators_action_data.status == ValidatorsActionStatus::Confirmed &&
                    validators_action_data.action_type.is_activation()
                {
                    // Activation is complete - move to Actioned for cleanup
                    <ValidatorActions<T>>::mutate(
                        &action_account_id,
                        ingress_counter,
                        |validators_action_data_maybe| {
                            if let Some(validators_action_data) = validators_action_data_maybe {
                                validators_action_data.status = ValidatorsActionStatus::Actioned
                            }
                        },
                    );
                } else if validators_action_data.status == ValidatorsActionStatus::Actioned {
                    // Remove completed actions to prevent storage bloat
                    <ValidatorActions<T>>::remove(&action_account_id, ingress_counter);
                }
            }
        }
    }
}

/// We use accountId for validatorId for simplicity
pub struct ValidatorOf<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> Convert<T::AccountId, Option<T::AccountId>> for ValidatorOf<T> {
    fn convert(account: T::AccountId) -> Option<T::AccountId> {
        Some(account)
    }
}

impl Default for ValidatorsActionStatus {
    fn default() -> Self {
        ValidatorsActionStatus::None
    }
}

impl Default for ValidatorsActionType {
    fn default() -> Self {
        ValidatorsActionType::Unknown
    }
}

impl<T: Config> EthereumPublicKeyChecker<T::AccountId> for Pallet<T> {
    fn get_validator_for_eth_public_key(eth_public_key: &ecdsa::Public) -> Option<T::AccountId> {
        Self::get_validator_by_eth_public_key(eth_public_key)
    }
}

impl<T: Config> DisabledValidatorChecker<T::AccountId> for Pallet<T> {
    fn is_disabled(validator_account_id: &T::AccountId) -> bool {
        Self::has_active_slash(validator_account_id)
    }
}

impl<T: Config> Enforcer<<T as session::Config>::ValidatorId> for Pallet<T> {
    fn slash_validator(
        slashed_validator_id: &<T as session::Config>::ValidatorId,
    ) -> DispatchResult {
        log::error!("❌ Error: Incomplete Slashing Implementation. An attempt was made to slash validator {:?}, but the slashing implementation is currently incomplete. This code path should not have been reached.", slashed_validator_id);
        Ok(())
    }
}
