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

pub type EthereumTransactionId = u32;

use frame_support::{
    dispatch::{DispatchResult, DispatchResultWithPostInfo}, ensure, pallet_prelude::StorageVersion, traits::Get, transactional,
};
use frame_system::{offchain::SendTransactionTypes, RawOrigin};
use pallet_session::{self as session, Config as SessionConfig};
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{Convert, Member},
    DispatchError,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::prelude::*;

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_avn::{
    self as avn, AccountToBytesConverter, BridgeInterfaceNotification, DisabledValidatorChecker,
    Enforcer, EthereumPublicKeyChecker, NewSessionHandler, ProcessedEventsChecker,
    ValidatorRegistrationNotifier, MAX_VALIDATOR_ACCOUNTS,
};

use sp_avn_common::{
    bounds::MaximumValidatorsBound, eth_key_actions::decompress_eth_public_key,
    event_types::Validator, BridgeContractMethod, IngressCounter,
};
use sp_core::{bounded::BoundedVec, ecdsa, H512};

pub use pallet_parachain_staking::{self as parachain_staking, BalanceOf, PositiveImbalanceOf};

use pallet_avn::BridgeInterface;

pub use pallet::*;

const PALLET_ID: &'static [u8; 14] = b"validators_mgr"; // Changed to fix the critical bug

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct PendingValidatorRegistrationData<AccountId, BlockNumber, Balance> {
    pub account_id: AccountId,
    pub eth_public_key: ecdsa::Public,
    pub tx_id: EthereumTransactionId,
    pub timestamp: BlockNumber,
    pub deposit: Option<Balance>,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct PendingValidatorDeregistrationData<BlockNumber> {
    pub tx_id: EthereumTransactionId,
    pub timestamp: BlockNumber,
    pub reason: ValidatorDeregistrationReason,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum PendingValidatorOperationType {
    Registration,
    Deregistration,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum ValidatorDeregistrationReason {
    Voluntary,
    Slashing,
    Governance,
}

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
        /// Pending registration not found
        PendingRegistrationNotFound,
        /// Pending deregistration not found
        PendingDeregistrationNotFound,
        /// Unknown transaction ID
        UnknownTransaction,
        /// Pending operation already exists for this account
        PendingOperationExists,
        /// T1 transaction failed
        T1TransactionFailed,
        /// Operation has timed out
        OperationTimeout,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ValidatorRegistered { validator_id: T::AccountId, eth_key: ecdsa::Public },
        ValidatorDeregistered { validator_id: T::AccountId },
        ValidatorActivationStarted { validator_id: T::AccountId },
        ValidatorActionConfirmed { action_id: ActionId<T::AccountId> },
        ValidatorSlashed { action_id: ActionId<T::AccountId> },
        PublishingValidatorActionOnEthereumFailed { tx_id: u32 },
        PublishingValidatorActionOnEthereumSucceeded { tx_id: u32 },
        /// Validator registration is pending T1 confirmation. \[validator_id, eth_key, tx_id\]
        ValidatorRegistrationPending { validator_id: T::AccountId, eth_key: ecdsa::Public, tx_id: u32 },
        /// Validator registration failed on T1. \[validator_id, tx_id\]
        ValidatorRegistrationFailed { validator_id: T::AccountId, tx_id: u32 },
        /// Validator deregistration is pending T1 confirmation. \[validator_id, tx_id\]
        ValidatorDeregistrationPending { validator_id: T::AccountId, tx_id: u32 },
        /// Validator deregistration failed on T1. \[validator_id, tx_id\]
        ValidatorDeregistrationFailed { validator_id: T::AccountId, tx_id: u32 },
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

    /// Pending validator registration requests awaiting T1 confirmation
    #[pallet::storage]
    pub type PendingValidatorRegistrations<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        PendingValidatorRegistrationData<T::AccountId, BlockNumberFor<T>, BalanceOf<T>>,
        OptionQuery,
    >;

    /// Pending validator deregistration requests awaiting T1 confirmation
    #[pallet::storage]
    pub type PendingValidatorDeregistrations<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        PendingValidatorDeregistrationData<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// Transaction ID to Account ID mapping for pending validator operations
    #[pallet::storage]
    pub type PendingValidatorTransactions<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        EthereumTransactionId,
        (T::AccountId, PendingValidatorOperationType),
        OptionQuery,
    >;

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
        /// This will now send a T1 transaction first and wait for confirmation before making any local changes.
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::add_collator())]
        #[transactional]
        pub fn add_collator(
            origin: OriginFor<T>,
            collator_account_id: T::AccountId,
            collator_eth_public_key: ecdsa::Public,
            deposit: Option<BalanceOf<T>>,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // Validate the registration request (includes pending operation check)
            Self::validate_validator_registration_request(&collator_account_id, &collator_eth_public_key)?;

            // Resolve deposit amount
            let final_deposit = deposit
                .or_else(|| Some(parachain_staking::Pallet::<T>::min_collator_stake()))
                .expect("has default value");

            // Send T1 transaction FIRST - no local state changes yet
            let tx_id = Self::send_validator_registration_to_t1(&collator_account_id, &collator_eth_public_key)?;

            // Store as pending (NO staking operations or active list changes yet)
            Self::store_pending_validator_registration(
                &collator_account_id, 
                &collator_eth_public_key, 
                Some(final_deposit), 
                tx_id
            )?;

            // Emit pending event - registration not confirmed yet
            Self::deposit_event(Event::<T>::ValidatorRegistrationPending {
                validator_id: collator_account_id,
                eth_key: collator_eth_public_key,
                tx_id
            });

            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_validator(MAX_VALIDATOR_ACCOUNTS))]
        #[transactional]
        pub fn remove_validator(
            origin: OriginFor<T>,
            collator_account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // Validate the deregistration request
            Self::validate_validator_deregistration_request(&collator_account_id)?;

            // Send T1 transaction FIRST - no local state changes yet
            let tx_id = Self::send_validator_deregistration_to_t1(&collator_account_id)?;

            // Store as pending deregistration (validator stays active for now, no staking changes yet)
            Self::store_pending_validator_deregistration(
                &collator_account_id, 
                tx_id, 
                ValidatorDeregistrationReason::Voluntary
            )?;

            // Emit pending event - deregistration not confirmed yet
            Self::deposit_event(Event::<T>::ValidatorDeregistrationPending {
                validator_id: collator_account_id,
                tx_id
            });

            Ok(())
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
            return Ok(())
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
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ValidatorsActionData {
    pub status: ValidatorsActionStatus,
    pub eth_transaction_id: EthereumTransactionId,
    pub action_type: ValidatorsActionType,
}

impl ValidatorsActionData {
    fn new(
        status: ValidatorsActionStatus,
        eth_transaction_id: EthereumTransactionId,
        action_type: ValidatorsActionType,
    ) -> Self {
        return ValidatorsActionData { status, eth_transaction_id, action_type }
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

// TODO [TYPE: review][PRI: medium]: if needed, make this the default value to a configurable
// option. See MinimumValidatorCount in Staking pallet as a reference
const DEFAULT_MINIMUM_VALIDATORS_COUNT: usize = 2;

pub type AVN<T> = avn::Pallet<T>;

impl<T: Config> Pallet<T> {
    fn start_activation_for_registered_validator(
        registered_validator: &T::AccountId,
        tx_id: EthereumTransactionId,
    ) {
        let ingress_counter = Self::get_ingress_counter() + 1;

        TotalIngresses::<T>::put(ingress_counter);
        <ValidatorActions<T>>::insert(
            registered_validator,
            ingress_counter,
            ValidatorsActionData::new(
                ValidatorsActionStatus::AwaitingConfirmation,
                tx_id,
                ValidatorsActionType::Activation,
            ),
        );
    }

    fn register_author(
        collator_account_id: &T::AccountId,
        collator_eth_public_key: &ecdsa::Public,
    ) -> DispatchResult {
        let decompressed_eth_public_key = decompress_eth_public_key(*collator_eth_public_key)
            .map_err(|_| Error::<T>::InvalidPublicKey)?;
        let validator_id_bytes =
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(collator_account_id);
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
        .map_err(|e| DispatchError::Other(e.into()))?;

        let new_collator_id =
            <T as SessionConfig>::ValidatorIdOf::convert(collator_account_id.clone())
                .ok_or(Error::<T>::ErrorConvertingAccountIdToValidatorId)?;

        Self::start_activation_for_registered_validator(collator_account_id, tx_id);
        T::ValidatorRegistrationNotifier::on_validator_registration(&new_collator_id);

        Self::deposit_event(Event::<T>::ValidatorRegistered {
            validator_id: collator_account_id.clone(),
            eth_key: *collator_eth_public_key,
        });
        Ok(())
    }

    /// We assume the full public key doesn't have the `04` prefix
    #[allow(dead_code)]
    fn compress_eth_public_key(full_public_key: H512) -> ecdsa::Public {
        let mut compressed_public_key = [0u8; 33];

        // Take bytes 0..32 from the full plublic key ()
        compressed_public_key[1..=32].copy_from_slice(&full_public_key.0[0..32]);
        // If the last byte of the full public key is even, prefix compresssed public key with 2,
        // otherwise prefix with 3
        compressed_public_key[0] = if full_public_key.0[63] % 2 == 0 { 2u8 } else { 3u8 };

        return ecdsa::Public::from_raw(compressed_public_key)
    }

    fn remove(
        validator_id: &T::AccountId,
        ingress_counter: IngressCounter,
        action_type: ValidatorsActionType,
        eth_public_key: ecdsa::Public,
    ) -> DispatchResult {
        let mut validator_account_ids =
            Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;

        ensure!(
            Self::get_ingress_counter() + 1 == ingress_counter,
            Error::<T>::InvalidIngressCounter
        );
        ensure!(
            validator_account_ids.len() > DEFAULT_MINIMUM_VALIDATORS_COUNT,
            Error::<T>::MinimumValidatorsReached
        );
        ensure!(
            !<ValidatorActions<T>>::contains_key(validator_id, ingress_counter),
            Error::<T>::RemovalAlreadyRequested
        );

        let maybe_validator_index = validator_account_ids.iter().position(|v| v == validator_id);
        if maybe_validator_index.is_none() {
            // Exit early if deregistration is not in the system. As dicussed, we don't want to give
            // any feedback if the validator is not found.
            return Ok(())
        }

        let index_of_validator_to_remove = maybe_validator_index.expect("checked for none already");

        let decompressed_eth_public_key =
            decompress_eth_public_key(eth_public_key).map_err(|_| Error::<T>::InvalidPublicKey)?;

        let validator_id_bytes =
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(validator_id);

        let function_name = BridgeContractMethod::RemoveAuthor.name_as_bytes();
        let params = vec![
            (b"bytes32".to_vec(), validator_id_bytes.to_vec()),
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
        ];
        let tx_id = <T as pallet::Config>::BridgeInterface::publish(
            function_name,
            &params,
            PALLET_ID.to_vec(),
        )
        .map_err(|e| DispatchError::Other(e.into()))?;

        TotalIngresses::<T>::put(ingress_counter);
        <ValidatorActions<T>>::insert(
            validator_id,
            ingress_counter,
            ValidatorsActionData::new(
                ValidatorsActionStatus::AwaitingConfirmation,
                tx_id,
                action_type,
            ),
        );
        validator_account_ids.swap_remove(index_of_validator_to_remove);
        <ValidatorAccountIds<T>>::put(validator_account_ids);

        Ok(())
    }

    fn remove_ethereum_public_key_if_required(validator_id: &T::AccountId) {
        let public_key_to_remove = Self::get_ethereum_public_key_if_exists(&validator_id);
        if let Some(public_key_to_remove) = public_key_to_remove {
            <EthereumPublicKeys<T>>::remove(public_key_to_remove);
        }
    }

    fn get_ethereum_public_key_if_exists(account_id: &T::AccountId) -> Option<ecdsa::Public> {
        return <EthereumPublicKeys<T>>::iter()
            .filter(|(_, acc)| acc == account_id)
            .map(|(pk, _)| pk)
            .nth(0)
    }

    fn validator_permanently_removed(
        active_validators: &Vec<Validator<T::AuthorityId, T::AccountId>>,
        disabled_validators: &Vec<T::AccountId>,
        deregistered_validator: &T::AccountId,
    ) -> bool {
        // If the validator exists in either vectors then they have not been removed from the
        // session
        return !active_validators.iter().any(|v| &v.account_id == deregistered_validator) &&
            !disabled_validators.iter().any(|v| v == deregistered_validator)
    }

    fn remove_deregistered_validator(resigned_validator: &T::AccountId) -> DispatchResult {
        // Take key from map.
        let t1_eth_public_key = match Self::get_ethereum_public_key_if_exists(resigned_validator) {
            Some(eth_public_key) => eth_public_key,
            _ => Err(Error::<T>::ValidatorNotFound)?,
        };

        let ingress_counter = Self::get_ingress_counter() + 1;
        return Self::remove(
            resigned_validator,
            ingress_counter,
            ValidatorsActionType::Resignation,
            t1_eth_public_key,
        )
    }

    fn deregistration_state_is_active(status: ValidatorsActionStatus) -> bool {
        return vec![ValidatorsActionStatus::AwaitingConfirmation, ValidatorsActionStatus::Confirmed]
            .contains(&status)
    }

    fn has_active_slash(validator_account_id: &T::AccountId) -> bool {
        return <ValidatorActions<T>>::iter_prefix_values(validator_account_id).any(
            |validators_action_data| {
                validators_action_data.action_type == ValidatorsActionType::Slashed &&
                    Self::deregistration_state_is_active(validators_action_data.status)
            },
        )
    }
    fn clean_up_staking_data(action_account_id: T::AccountId) -> Result<(), ()> {
        // Cleanup staking state for the collator we are removing
        let staking_state = parachain_staking::Pallet::<T>::candidate_info(&action_account_id);
        if staking_state.is_none() {
            log::error!(
                "💔 Unable to find staking candidate info for collator: {:?}",
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
                "💔 Error removing staking data for collator {:?}: {:?}",
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

    /// Handle the result of a validator registration request sent to T1
    fn handle_validator_registration_result(
        account_id: T::AccountId,
        tx_id: EthereumTransactionId,
        succeeded: bool,
    ) -> DispatchResult {
        let pending_data = PendingValidatorRegistrations::<T>::get(&account_id)
            .ok_or(Error::<T>::PendingRegistrationNotFound)?;

        if succeeded {
            // T1 confirmed - proceed with actual registration
            log::info!("✅ T1 confirmed registration for validator {:?}", account_id);
            Self::complete_validator_registration(&account_id, &pending_data)?;
        } else {
            // T1 failed - cleanup pending request
            log::error!("❌ T1 failed registration for validator {:?}", account_id);
            Self::deposit_event(Event::<T>::ValidatorRegistrationFailed {
                validator_id: account_id.clone(),
                tx_id,
            });
        }

        // Remove pending registration
        PendingValidatorRegistrations::<T>::remove(&account_id);
        Ok(())
    }

    /// Handle the result of a validator deregistration request sent to T1
    fn handle_validator_deregistration_result(
        account_id: T::AccountId,
        tx_id: EthereumTransactionId,
        succeeded: bool,
    ) -> DispatchResult {
        let _pending_data = PendingValidatorDeregistrations::<T>::get(&account_id)
            .ok_or(Error::<T>::PendingDeregistrationNotFound)?;

        if succeeded {
            // T1 confirmed - proceed with actual deregistration
            log::info!("✅ T1 confirmed deregistration for validator {:?}", account_id);
            Self::complete_validator_deregistration(&account_id)?;
        } else {
            // T1 failed - validator stays active
            log::error!("❌ T1 failed deregistration for validator {:?}", account_id);
            Self::deposit_event(Event::<T>::ValidatorDeregistrationFailed {
                validator_id: account_id.clone(),
                tx_id,
            });
        }

        // Remove pending deregistration
        PendingValidatorDeregistrations::<T>::remove(&account_id);
        Ok(())
    }

    /// Store a pending validator registration request
    fn store_pending_validator_registration(
        account_id: &T::AccountId,
        eth_public_key: &ecdsa::Public,
        deposit: Option<BalanceOf<T>>,
        tx_id: EthereumTransactionId,
    ) -> DispatchResult {
        let current_block = <frame_system::Pallet<T>>::block_number();
        let pending_data = PendingValidatorRegistrationData {
            account_id: account_id.clone(),
            eth_public_key: *eth_public_key,
            tx_id,
            timestamp: current_block,
            deposit,
        };

        PendingValidatorRegistrations::<T>::insert(account_id, pending_data);
        PendingValidatorTransactions::<T>::insert(tx_id, (account_id.clone(), PendingValidatorOperationType::Registration));
        
        Ok(())
    }

    /// Store a pending validator deregistration request
    fn store_pending_validator_deregistration(
        account_id: &T::AccountId,
        tx_id: EthereumTransactionId,
        reason: ValidatorDeregistrationReason,
    ) -> DispatchResult {
        let current_block = <frame_system::Pallet<T>>::block_number();
        let pending_data = PendingValidatorDeregistrationData {
            tx_id,
            timestamp: current_block,
            reason,
        };

        PendingValidatorDeregistrations::<T>::insert(account_id, pending_data);
        PendingValidatorTransactions::<T>::insert(tx_id, (account_id.clone(), PendingValidatorOperationType::Deregistration));
        
        Ok(())
    }

    /// Validate a validator registration request
    fn validate_validator_registration_request(
        account_id: &T::AccountId,
        eth_public_key: &ecdsa::Public,
    ) -> DispatchResult {
        // Check if there's already a pending operation for this account
        ensure!(
            !PendingValidatorRegistrations::<T>::contains_key(account_id) &&
            !PendingValidatorDeregistrations::<T>::contains_key(account_id),
            Error::<T>::PendingOperationExists
        );

        let validator_account_ids = Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;
        ensure!(!validator_account_ids.is_empty(), Error::<T>::NoValidators);

        ensure!(
            !validator_account_ids.contains(account_id),
            Error::<T>::ValidatorAlreadyExists
        );
        
        ensure!(
            !<EthereumPublicKeys<T>>::contains_key(eth_public_key),
            Error::<T>::ValidatorEthKeyAlreadyExists
        );

        ensure!(
            validator_account_ids.len() < (<MaximumValidatorsBound as sp_core::TypedGet>::get() as usize),
            Error::<T>::MaximumValidatorsReached
        );

        Ok(())
    }

    /// Validate a validator deregistration request
    fn validate_validator_deregistration_request(account_id: &T::AccountId) -> DispatchResult {
        // Check if there's already a pending operation for this account
        ensure!(
            !PendingValidatorRegistrations::<T>::contains_key(account_id) &&
            !PendingValidatorDeregistrations::<T>::contains_key(account_id),
            Error::<T>::PendingOperationExists
        );

        let validator_account_ids = Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;
        
        ensure!(
            validator_account_ids.len() > DEFAULT_MINIMUM_VALIDATORS_COUNT,
            Error::<T>::MinimumValidatorsReached
        );

        ensure!(
            validator_account_ids.contains(account_id),
            Error::<T>::ValidatorNotFound
        );

        Ok(())
    }

    /// Send validator registration request to T1
    fn send_validator_registration_to_t1(
        validator_account_id: &T::AccountId,
        validator_eth_public_key: &ecdsa::Public,
    ) -> Result<EthereumTransactionId, DispatchError> {
        let decompressed_eth_public_key = decompress_eth_public_key(*validator_eth_public_key)
            .map_err(|_| Error::<T>::InvalidPublicKey)?;

        let validator_id_bytes = <T as pallet::Config>::AccountToBytesConvert::into_bytes(validator_account_id);

        let function_name = BridgeContractMethod::AddAuthor.name_as_bytes();
        let params = vec![
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
            (b"bytes32".to_vec(), validator_id_bytes.to_vec()),
        ];

        <T as pallet::Config>::BridgeInterface::publish(
            function_name,
            &params,
            PALLET_ID.to_vec(),
        )
        .map_err(|e| DispatchError::Other(e.into()))
    }

    /// Send validator deregistration request to T1
    fn send_validator_deregistration_to_t1(
        validator_account_id: &T::AccountId,
    ) -> Result<EthereumTransactionId, DispatchError> {
        let eth_public_key = Self::get_ethereum_public_key_if_exists(validator_account_id)
            .ok_or(Error::<T>::ValidatorNotFound)?;

        let decompressed_eth_public_key = decompress_eth_public_key(eth_public_key)
            .map_err(|_| Error::<T>::InvalidPublicKey)?;

        let validator_id_bytes = <T as pallet::Config>::AccountToBytesConvert::into_bytes(validator_account_id);

        let function_name = BridgeContractMethod::RemoveAuthor.name_as_bytes();
        let params = vec![
            (b"bytes32".to_vec(), validator_id_bytes.to_vec()),
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
        ];

        <T as pallet::Config>::BridgeInterface::publish(
            function_name,
            &params,
            PALLET_ID.to_vec(),
        )
        .map_err(|e| DispatchError::Other(e.into()))
    }

    /// Complete validator registration after T1 confirmation
    fn complete_validator_registration(
        account_id: &T::AccountId,
        pending_data: &PendingValidatorRegistrationData<T::AccountId, BlockNumberFor<T>, BalanceOf<T>>,
    ) -> DispatchResult {
        // Execute staking operations first
        if let Some(deposit) = pending_data.deposit {
            let candidate_count = parachain_staking::Pallet::<T>::candidate_pool().0.len() as u32;
            match parachain_staking::Pallet::<T>::join_candidates(
                <T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(
                    account_id.clone(),
                )),
                deposit,
                candidate_count,
            ) {
                Ok(_) => {},
                Err(e) => return Err(e.error),
            }
        }

        // Add to active validators list
        <ValidatorAccountIds<T>>::try_append(account_id.clone())
            .map_err(|_| Error::<T>::MaximumValidatorsReached)?;

        // Add ethereum key mapping
        <EthereumPublicKeys<T>>::insert(pending_data.eth_public_key, account_id);

        // Notify validator registration
        let new_validator_id = <T as SessionConfig>::ValidatorIdOf::convert(account_id.clone())
            .ok_or(Error::<T>::ErrorConvertingAccountIdToValidatorId)?;
        T::ValidatorRegistrationNotifier::on_validator_registration(&new_validator_id);

        // Emit success event
        Self::deposit_event(Event::<T>::ValidatorRegistered {
            validator_id: account_id.clone(),
            eth_key: pending_data.eth_public_key,
        });

        Ok(())
    }

    /// Complete validator deregistration after T1 confirmation
    fn complete_validator_deregistration(account_id: &T::AccountId) -> DispatchResult {
        // Execute staking cleanup first
        let candidate_count = parachain_staking::Pallet::<T>::candidate_pool().0.len() as u32;
        match parachain_staking::Pallet::<T>::schedule_leave_candidates(
            <T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(
                account_id.clone(),
            )),
            candidate_count,
        ) {
            Ok(_) => {},
            Err(e) => return Err(e.error),
        }

        // Remove from active validators
        let mut validator_account_ids = Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;
        let index = validator_account_ids.iter().position(|v| v == account_id)
            .ok_or(Error::<T>::ValidatorNotFound)?;
        validator_account_ids.swap_remove(index);
        <ValidatorAccountIds<T>>::put(validator_account_ids);

        // Remove ethereum key mapping
        Self::remove_ethereum_public_key_if_required(account_id);

        // Emit success event
        Self::deposit_event(Event::<T>::ValidatorDeregistered {
            validator_id: account_id.clone(),
        });

        Ok(())
    }
}

impl<T: Config> BridgeInterfaceNotification for Pallet<T> {
    fn process_result(tx_id: u32, caller_id: Vec<u8>, succeeded: bool) -> DispatchResult {
        if caller_id != PALLET_ID.to_vec() {
            return Ok(())
        }

        // Check if this is a pending operation transaction first
        if let Some((account_id, operation_type)) = PendingValidatorTransactions::<T>::get(tx_id) {
            match operation_type {
                PendingValidatorOperationType::Registration => {
                    Self::handle_validator_registration_result(account_id, tx_id, succeeded)?;
                },
                PendingValidatorOperationType::Deregistration => {
                    Self::handle_validator_deregistration_result(account_id, tx_id, succeeded)?;
                },
            }
            // Cleanup transaction mapping
            PendingValidatorTransactions::<T>::remove(tx_id);
        } else {
            // Fall back to existing transaction processing for legacy operations
            if succeeded {
                log::info!(
                    "✅  Transaction with ID {} was successfully published to Ethereum.",
                    tx_id
                );
                Self::deposit_event(Event::<T>::PublishingValidatorActionOnEthereumSucceeded {
                    tx_id,
                });
            } else {
                log::error!("❌ Transaction with ID {} failed to publish to Ethereum.", tx_id);
                Self::deposit_event(Event::<T>::PublishingValidatorActionOnEthereumFailed {
                    tx_id,
                });
            }
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
        return ActionId::<AccountId> { action_account_id, ingress_counter }
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
                }
            }
        }
    }
}

/// We use accountId for validatorId for simplicity
pub struct ValidatorOf<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> Convert<T::AccountId, Option<T::AccountId>> for ValidatorOf<T> {
    fn convert(account: T::AccountId) -> Option<T::AccountId> {
        return Some(account)
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
        return Self::has_active_slash(validator_account_id)
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
