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
    pallet_prelude::{DispatchClass, StorageVersion, Weight},
    traits::{Currency, Get},
    transactional,
};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::{offchain::SendTransactionTypes, RawOrigin};
use pallet_session::{self as session, Config as SessionConfig};
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{Convert, Member, Saturating},
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
    bounds::MaximumValidatorsBound, eth_key_actions::decompress_eth_public_key,
    event_types::Validator, BridgeContractMethod, IngressCounter,
};
use sp_core::{bounded::BoundedVec, ecdsa};

pub use pallet_parachain_staking::{self as parachain_staking, BalanceOf, PositiveImbalanceOf};

use pallet_avn::BridgeInterface;

pub use pallet::*;

const PALLET_ID: &'static [u8; 14] = b"author_manager";

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct PendingValidatorRegistrationData<AccountId, BlockNumber, Balance> {
    pub account_id: AccountId,
    pub eth_public_key: ecdsa::Public,
    pub tx_id: EthereumId,
    pub timestamp: BlockNumber,
    pub deposit: Option<Balance>,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct PendingValidatorDeregistrationData<BlockNumber> {
    pub tx_id: EthereumId,
    pub timestamp: BlockNumber,
    pub reason: ValidatorDeregistrationReason,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct DeactivationData<BlockNumber> {
    pub scheduled_block: BlockNumber,
    pub reason: ValidatorDeregistrationReason,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum PendingValidatorOperationType {
    Registration,
    Deregistration,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo)]
pub enum ValidatorStatus {
    NotValidator,
    PendingRegistration,
    Active,
    PendingDeregistration,
    Deactivating,
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

        /// Maximum blocks a pending operation can remain before timeout
        #[pallet::constant]
        type PendingOperationTimeout: Get<BlockNumberFor<Self>>;

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
        /// The ethereum public key of this validator already exists
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
        // New errors
        InsufficientBalance,
        AlreadyCandidate,
        DepositBelowMinimum,
        MaxCandidatesReached,
        CandidateLeaving,
        ValidatorDeactivating,
        SessionKeysNotSet,
        RegistrationInProgress,
        DeregistrationInProgress,
        ActivationInProgress,
        BridgePublishFailed,
        NoPendingOperationFound,
        /// Operation was already processed
        AlreadyProcessed,
        /// Operation is too recent for force completion
        OperationTooRecent,
        /// Retry attempt is too soon
        RetryTooSoon,
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
        /// Validator registration is pending T1 confirmation. \[validator_id, eth_key, tx_id\]
        ValidatorRegistrationPending {
            validator_id: T::AccountId,
            eth_key: ecdsa::Public,
            tx_id: u32,
        },
        /// Validator registration failed on T1. \[validator_id, tx_id\]
        ValidatorRegistrationFailed {
            validator_id: T::AccountId,
            tx_id: u32,
        },
        /// Validator deregistration is pending T1 confirmation. \[validator_id, tx_id\]
        ValidatorDeregistrationPending {
            validator_id: T::AccountId,
            tx_id: u32,
        },
        /// Validator deregistration failed on T1. \[validator_id, tx_id\]
        ValidatorDeregistrationFailed {
            validator_id: T::AccountId,
            tx_id: u32,
        },
        ValidatorRegistrationTimedOut {
            validator_id: T::AccountId,
            tx_id: u32,
        },
        ValidatorDeregistrationTimedOut {
            validator_id: T::AccountId,
            tx_id: u32,
        },
        ValidatorOperationCancelled {
            validator_id: T::AccountId,
            operation_type: PendingValidatorOperationType,
        },
        ValidatorDeregistrationScheduled {
            validator_id: T::AccountId,
        },
        UnknownTransactionCallback {
            tx_id: u32,
        },
        StakingOperationFailed {
            validator_id: T::AccountId,
        },
        T1TransactionSent {
            validator_id: T::AccountId,
            tx_id: u32,
            operation_type: PendingValidatorOperationType,
        },
        PendingOperationsQueried {
            count: u32,
        },
        OperationRetried {
            validator_id: T::AccountId,
            old_tx_id: u32,
            new_tx_id: u32,
        },
        OperationForcedComplete {
            validator_id: T::AccountId,
            operation_type: PendingValidatorOperationType,
        },
        /// T1 succeeded but local completion failed (allows retry via force_complete)
        ValidatorDeregistrationCompletionFailed {
            validator_id: T::AccountId,
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
        EthereumId,
        (T::AccountId, PendingValidatorOperationType),
        OptionQuery,
    >;

    /// Validators that are in the process of leaving
    #[pallet::storage]
    pub type DeactivatingValidators<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        DeactivationData<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// Transactions that have been processed (to prevent replay)
    #[pallet::storage]
    pub type ProcessedTransactions<T: Config> =
        StorageMap<_, Blake2_128Concat, EthereumId, BlockNumberFor<T>, OptionQuery>;

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

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            // Calculate maximum weight budget (10% of max block weight)
            let max_weight = T::BlockWeights::get()
                .get(DispatchClass::Normal)
                .max_total
                .unwrap_or(Weight::from_parts(2_000_000_000_000, 0))
                .saturating_div(10);

            let mut remaining_weight = max_weight;

            // Check for timed out registrations
            if remaining_weight.ref_time() > 0 {
                let used = Self::cleanup_timed_out_registrations(n, remaining_weight);
                remaining_weight = remaining_weight.saturating_sub(used);
            }

            // Check for timed out deregistrations
            if remaining_weight.ref_time() > 0 {
                let used = Self::cleanup_timed_out_deregistrations(n, remaining_weight);
                remaining_weight = remaining_weight.saturating_sub(used);
            }

            // Check for validators ready to be fully removed
            if remaining_weight.ref_time() > 0 {
                let used = Self::finalize_deactivated_validators(n, remaining_weight);
                remaining_weight = remaining_weight.saturating_sub(used);
            }

            // Cleanup old processed transactions
            if remaining_weight.ref_time() > 0 {
                let used = Self::cleanup_old_processed_transactions(n, remaining_weight);
                remaining_weight = remaining_weight.saturating_sub(used);
            }

            max_weight.saturating_sub(remaining_weight)
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

            // Resolve deposit amount
            let final_deposit =
                deposit.unwrap_or_else(|| parachain_staking::Pallet::<T>::min_collator_stake());

            // Validate the registration request (includes staking preconditions)
            Self::validate_validator_registration_request(
                &collator_account_id,
                &collator_eth_public_key,
                final_deposit,
            )?;

            // Send T1 transaction FIRST to get tx_id
            // eth-bridge processes sequentially, so this returns immediately with tx_id
            let tx_id = Self::send_validator_registration_to_t1(
                &collator_account_id,
                &collator_eth_public_key,
            )?;

            // Store pending state with actual tx_id (single atomic operation)
            Self::store_pending_validator_registration(
                &collator_account_id,
                &collator_eth_public_key,
                Some(final_deposit),
                tx_id,
            )?;

            // Emit pending event
            Self::deposit_event(Event::<T>::ValidatorRegistrationPending {
                validator_id: collator_account_id.clone(),
                eth_key: collator_eth_public_key,
                tx_id,
            });

            Self::deposit_event(Event::<T>::T1TransactionSent {
                validator_id: collator_account_id,
                tx_id,
                operation_type: PendingValidatorOperationType::Registration,
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

            // Send T1 transaction FIRST to get tx_id
            // eth-bridge processes sequentially, so this returns immediately with tx_id
            let tx_id = Self::send_validator_deregistration_to_t1(&collator_account_id)?;

            // Store pending state with actual tx_id (single atomic operation)
            Self::store_pending_validator_deregistration(
                &collator_account_id,
                tx_id,
                ValidatorDeregistrationReason::Voluntary,
            )?;

            // Emit pending event
            Self::deposit_event(Event::<T>::ValidatorDeregistrationPending {
                validator_id: collator_account_id.clone(),
                tx_id,
            });

            Self::deposit_event(Event::<T>::T1TransactionSent {
                validator_id: collator_account_id,
                tx_id,
                operation_type: PendingValidatorOperationType::Deregistration,
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
            ensure_root(origin)?;

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

        /// Force complete a validator registration that succeeded on T1 but failed locally.
        ///
        /// This is an admin recovery function. Only use when:
        /// 1. T1 transaction succeeded (validator added to bridge contract)
        /// 2. Local completion failed (validator not in ValidatorAccountIds)
        /// 3. Pending registration state exists
        ///
        /// **WARNING:** Only call this if you've verified T1 state manually.
        /// Calling this when T1 failed will cause inconsistency.
        ///
        /// ## Parameters
        /// - `origin`: Must be root (sudo)
        /// - `account_id`: The account to complete registration for
        ///
        /// ## Errors
        /// - `PendingRegistrationNotFound`: No pending registration exists
        /// - `ValidatorAlreadyExists`: Validator is already active
        /// - Staking pallet errors if join_candidates fails
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::force_complete_registration())]
        #[transactional]
        pub fn force_complete_registration(
            origin: OriginFor<T>,
            account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // Get pending data
            let pending_data = PendingValidatorRegistrations::<T>::get(&account_id)
                .ok_or(Error::<T>::PendingRegistrationNotFound)?;

            // Verify not already active
            let validator_account_ids =
                Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;
            ensure!(
                !validator_account_ids.contains(&account_id),
                Error::<T>::ValidatorAlreadyExists
            );

            log::warn!(
                "⚠️  ADMIN ACTION: Force completing registration for {:?} (tx_id: {})",
                account_id,
                pending_data.tx_id
            );

            // Force complete
            Self::complete_validator_registration(&account_id, &pending_data)?;

            // Cleanup pending state
            frame_support::storage::transactional::with_transaction(|| {
                PendingValidatorRegistrations::<T>::remove(&account_id);
                PendingValidatorTransactions::<T>::remove(pending_data.tx_id);
                frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
            })?;

            Self::deposit_event(Event::<T>::OperationForcedComplete {
                validator_id: account_id.clone(),
                operation_type: PendingValidatorOperationType::Registration,
            });

            log::info!("✅ Force completed registration for {:?}", account_id);

            Ok(())
        }

        /// Force complete a validator deregistration that succeeded on T1 but failed locally.
        ///
        /// This is an admin recovery function. Only use when:
        /// 1. T1 transaction succeeded (validator removed from bridge contract)
        /// 2. Local completion failed (validator still in ValidatorAccountIds)
        /// 3. Pending deregistration state exists
        ///
        /// **WARNING:** Only call this if you've verified T1 state manually.
        /// Calling this when T1 failed will cause inconsistency.
        ///
        /// ## Parameters
        /// - `origin`: Must be root (sudo)
        /// - `account_id`: The account to complete deregistration for
        ///
        /// ## Special Cases
        /// - If validator already left staking pool: Just cleanup state
        /// - If validator is already leaving: Just mark as deactivating
        ///
        /// ## Errors
        /// - `PendingDeregistrationNotFound`: No pending deregistration exists
        /// - Staking pallet errors if schedule_leave_candidates fails
        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::force_complete_deregistration())]
        #[transactional]
        pub fn force_complete_deregistration(
            origin: OriginFor<T>,
            account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // Get pending data
            let pending_data = PendingValidatorDeregistrations::<T>::get(&account_id)
                .ok_or(Error::<T>::PendingDeregistrationNotFound)?;

            // Check if completing would violate minimum validator count
            if let Some(validator_account_ids) = Self::validator_account_ids() {
                let remaining_count = validator_account_ids.len().saturating_sub(1);
                let minimum = T::MinimumValidatorCount::get() as usize;
                
                if remaining_count < minimum {
                    log::error!(
                        "🚨 WARNING: Force completing deregistration will result in {} validators, \
                        below minimum of {}. Proceeding because this is admin recovery, \
                        but network security may be compromised!",
                        remaining_count,
                        minimum
                    );
                }
            }

            log::warn!(
                "ADMIN ACTION: Force completing deregistration for {:?} (tx_id: {})",
                account_id,
                pending_data.tx_id
            );

            // Check if validator already left (manual action or other process)
            if parachain_staking::Pallet::<T>::candidate_info(&account_id).is_none() {
                log::info!(
                    "Validator {:?} already not a candidate, performing cleanup only",
                    account_id
                );

                // Just cleanup state
                if let Some(mut validator_account_ids) = Self::validator_account_ids() {
                    if let Some(index) =
                        validator_account_ids.iter().position(|v| v == &account_id)
                    {
                        validator_account_ids.swap_remove(index);
                        <ValidatorAccountIds<T>>::put(validator_account_ids);
                    }
                }

                Self::remove_ethereum_public_key_if_required(&account_id);

                // Cleanup pending
                frame_support::storage::transactional::with_transaction(|| {
                    PendingValidatorDeregistrations::<T>::remove(&account_id);
                    PendingValidatorTransactions::<T>::remove(pending_data.tx_id);
                    frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(
                        (),
                    ))
                })?;

                Self::deposit_event(Event::<T>::ValidatorDeregistered {
                    validator_id: account_id.clone(),
                });
                Self::deposit_event(Event::<T>::OperationForcedComplete {
                    validator_id: account_id.clone(),
                    operation_type: PendingValidatorOperationType::Deregistration,
                });

                log::info!(
                    "✅ Force completed deregistration for {:?} (cleanup only)",
                    account_id
                );
                return Ok(());
            }

            // Try normal completion
            let result = Self::complete_validator_deregistration(&account_id);

            match result {
                Ok(_) => {
                    // Success - cleanup pending
                    frame_support::storage::transactional::with_transaction(|| {
                        PendingValidatorDeregistrations::<T>::remove(&account_id);
                        PendingValidatorTransactions::<T>::remove(pending_data.tx_id);
                        frame_support::storage::TransactionOutcome::Commit(Ok::<
                            (),
                            DispatchError,
                        >(()))
                    })?;

                    Self::deposit_event(Event::<T>::OperationForcedComplete {
                        validator_id: account_id.clone(),
                        operation_type: PendingValidatorOperationType::Deregistration,
                    });

                    log::info!("✅ Force completed deregistration for {:?}", account_id);
                    Ok(())
                },
                Err(e) => {
                    // If error is because validator is already leaving, that's ok
                    if let Some(candidate_info) =
                        parachain_staking::Pallet::<T>::candidate_info(&account_id)
                    {
                        if candidate_info.is_leaving() {
                            log::info!(
                                "Validator {:?} is already leaving, marking as deactivating",
                                account_id
                            );

                            // Mark as deactivating
                            DeactivatingValidators::<T>::insert(
                                &account_id,
                                DeactivationData {
                                    scheduled_block: <frame_system::Pallet<T>>::block_number(),
                                    reason: ValidatorDeregistrationReason::Voluntary,
                                },
                            );

                            Self::remove_ethereum_public_key_if_required(&account_id);

                            // Cleanup pending
                            frame_support::storage::transactional::with_transaction(|| {
                                PendingValidatorDeregistrations::<T>::remove(&account_id);
                                PendingValidatorTransactions::<T>::remove(pending_data.tx_id);
                                frame_support::storage::TransactionOutcome::Commit(Ok::<
                                    (),
                                    DispatchError,
                                >(()))
                            })?;

                            Self::deposit_event(Event::<T>::ValidatorDeregistrationScheduled {
                                validator_id: account_id.clone(),
                            });
                            Self::deposit_event(Event::<T>::OperationForcedComplete {
                                validator_id: account_id,
                                operation_type: PendingValidatorOperationType::Deregistration,
                            });

                            return Ok(());
                        }
                    }

                    // Real error
                    log::error!(
                        "❌ Failed to force complete deregistration for {:?}: {:?}",
                        account_id,
                        e
                    );
                    Err(e)
                },
            }
        }

        /// Cancel a pending validator operation.
        ///
        /// This is an admin recovery function. Use when:
        /// 1. T1 transaction timed out and will never complete
        /// 2. T1 transaction failed but callback was missed
        /// 3. Operation was initiated by mistake
        /// 4. Need to abort stuck operation
        ///
        /// **This function only removes pending state. It does NOT:**
        /// - Add or remove validators
        /// - Change staking state
        /// - Interact with T1
        ///
        /// After cancellation, a new operation can be attempted.
        ///
        /// ## Parameters
        /// - `origin`: Must be root (sudo)
        /// - `account_id`: The account to cancel operation for
        ///
        /// ## Errors
        /// - `NoPendingOperationFound`: No pending operation exists
        #[pallet::call_index(5)]
        #[pallet::weight(<T as Config>::WeightInfo::cancel_pending_operation())]
        #[transactional]
        pub fn cancel_pending_operation(
            origin: OriginFor<T>,
            account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            log::warn!(
                "ADMIN ACTION: Cancelling pending operation for {:?}",
                account_id
            );

            // Check for pending registration
            if let Some(pending_data) = PendingValidatorRegistrations::<T>::take(&account_id) {
                PendingValidatorTransactions::<T>::remove(pending_data.tx_id);

                Self::deposit_event(Event::<T>::ValidatorOperationCancelled {
                    validator_id: account_id.clone(),
                    operation_type: PendingValidatorOperationType::Registration,
                });

                log::info!("Cancelled pending registration for {:?}", account_id);
                return Ok(());
            }

            // Check for pending deregistration
            if let Some(pending_data) = PendingValidatorDeregistrations::<T>::take(&account_id) {
                PendingValidatorTransactions::<T>::remove(pending_data.tx_id);

                Self::deposit_event(Event::<T>::ValidatorOperationCancelled {
                    validator_id: account_id.clone(),
                    operation_type: PendingValidatorOperationType::Deregistration,
                });

                log::info!("Cancelled pending deregistration for {:?}", account_id);
                return Ok(());
            }

            // No pending operation found
            log::error!("❌ No pending operation found for {:?}", account_id);
            Err(Error::<T>::NoPendingOperationFound.into())
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

/// Maximum cleanup items per block to prevent weight overflow
const MAX_TIMEOUT_CLEANUPS_PER_BLOCK: usize = 10;
const MAX_DEACTIVATION_FINALIZATIONS_PER_BLOCK: usize = 5;
const MAX_PROCESSED_TX_CLEANUPS_PER_BLOCK: usize = 50;

pub type AVN<T> = avn::Pallet<T>;

impl<T: Config> Pallet<T> {
    fn cleanup_timed_out_registrations(
        current_block: BlockNumberFor<T>,
        max_weight: Weight,
    ) -> Weight {
        let timeout_threshold = T::PendingOperationTimeout::get();
        let mut weight = T::DbWeight::get().reads(1);

        // Check if we have enough weight to do anything
        if weight.ref_time() >= max_weight.ref_time() {
            return weight;
        }

        // BOUNDED: Limit both iterations AND results to prevent unbounded storage reads
        let mut timed_out = Vec::with_capacity(MAX_TIMEOUT_CLEANUPS_PER_BLOCK);
        let mut iterations = 0u32;
        const MAX_ITERATIONS: u32 = 100; // Safety limit: max storage reads per call

        for (account_id, pending_data) in PendingValidatorRegistrations::<T>::iter() {
            iterations += 1;

            // Safety: Stop if we've iterated too many times
            if iterations >= MAX_ITERATIONS {
                if timed_out.is_empty() {
                    log::warn!(
                        "⚠️  Reached iteration limit in cleanup_timed_out_registrations \
                        without finding any timed out operations"
                    );
                }
                break;
            }

            // Check if this entry is timed out
            if current_block.saturating_sub(pending_data.timestamp) > timeout_threshold {
                timed_out.push(account_id);
                // Stop if we have enough results
                if timed_out.len() >= MAX_TIMEOUT_CLEANUPS_PER_BLOCK {
                    break;
                }
            }
        }

        // Account for actual iterations, not just results
        weight = weight.saturating_add(T::DbWeight::get().reads(iterations as u64));

        for account_id in timed_out {
            // Check weight before each operation
            let operation_weight = T::DbWeight::get().reads_writes(1, 2);
            if weight.saturating_add(operation_weight).ref_time() > max_weight.ref_time() {
                break;
            }

            if let Some(pending_data) = PendingValidatorRegistrations::<T>::take(&account_id) {
                PendingValidatorTransactions::<T>::remove(pending_data.tx_id);

                Self::deposit_event(Event::<T>::ValidatorRegistrationTimedOut {
                    validator_id: account_id,
                    tx_id: pending_data.tx_id,
                });

                weight = weight.saturating_add(operation_weight);
            }
        }

        weight
    }

    fn cleanup_timed_out_deregistrations(
        current_block: BlockNumberFor<T>,
        max_weight: Weight,
    ) -> Weight {
        let timeout_threshold = T::PendingOperationTimeout::get();
        let mut weight = T::DbWeight::get().reads(1);

        if weight.ref_time() >= max_weight.ref_time() {
            return weight;
        }

        // BOUNDED: Limit both iterations AND results to prevent unbounded storage reads
        let mut timed_out = Vec::with_capacity(MAX_TIMEOUT_CLEANUPS_PER_BLOCK);
        let mut iterations = 0u32;
        const MAX_ITERATIONS: u32 = 100; // Safety limit: max storage reads per call

        for (account_id, pending_data) in PendingValidatorDeregistrations::<T>::iter() {
            iterations += 1;

            // Safety: Stop if we've iterated too many times
            if iterations >= MAX_ITERATIONS {
                if timed_out.is_empty() {
                    log::warn!(
                        "⚠️  Reached iteration limit in cleanup_timed_out_deregistrations \
                        without finding any timed out operations"
                    );
                }
                break;
            }

            // Check if this entry is timed out
            if current_block.saturating_sub(pending_data.timestamp) > timeout_threshold {
                timed_out.push(account_id);
                // Stop if we have enough results
                if timed_out.len() >= MAX_TIMEOUT_CLEANUPS_PER_BLOCK {
                    break;
                }
            }
        }

        // Account for actual iterations, not just results
        weight = weight.saturating_add(T::DbWeight::get().reads(iterations as u64));

        for account_id in timed_out {
            let operation_weight = T::DbWeight::get().reads_writes(1, 2);
            if weight.saturating_add(operation_weight).ref_time() > max_weight.ref_time() {
                break;
            }

            if let Some(pending_data) = PendingValidatorDeregistrations::<T>::take(&account_id) {
                PendingValidatorTransactions::<T>::remove(pending_data.tx_id);

                Self::deposit_event(Event::<T>::ValidatorDeregistrationTimedOut {
                    validator_id: account_id,
                    tx_id: pending_data.tx_id,
                });

                weight = weight.saturating_add(operation_weight);
            }
        }

        weight
    }

    fn finalize_deactivated_validators(
        _current_block: BlockNumberFor<T>,
        max_weight: Weight,
    ) -> Weight {
        let mut weight = T::DbWeight::get().reads(1);

        if weight.ref_time() >= max_weight.ref_time() {
            return weight;
        }

        // BOUNDED: Limit both iterations AND results to prevent unbounded storage reads
        let mut ready_to_remove = Vec::with_capacity(MAX_DEACTIVATION_FINALIZATIONS_PER_BLOCK);
        let mut iterations = 0u32;
        const MAX_ITERATIONS: u32 = 50; // Safety limit: max storage reads per call

        for (account_id, _deactivation_data) in DeactivatingValidators::<T>::iter() {
            iterations += 1;

            if iterations >= MAX_ITERATIONS {
                break;
            }

            // Check if they're still a candidate - if not, they've completed exit
            if parachain_staking::Pallet::<T>::candidate_info(&account_id).is_none() {
                ready_to_remove.push(account_id);
                if ready_to_remove.len() >= MAX_DEACTIVATION_FINALIZATIONS_PER_BLOCK {
                    break;
                }
            }
        }

        // Account for actual iterations (1 read per deactivating validator, 1 read per candidate_info check)
        weight = weight.saturating_add(T::DbWeight::get().reads(iterations as u64 * 2));

        for account_id in ready_to_remove {
            let operation_weight = T::DbWeight::get().reads_writes(2, 3);
            if weight.saturating_add(operation_weight).ref_time() > max_weight.ref_time() {
                break;
            }

            // Remove from ValidatorAccountIds with proper error handling
            if let Some(mut validator_account_ids) = Self::validator_account_ids() {
                if let Some(index) = validator_account_ids.iter().position(|v| v == &account_id) {
                    validator_account_ids.swap_remove(index);
                    <ValidatorAccountIds<T>>::put(validator_account_ids);
                }
            }

            // Always cleanup deactivating state
            DeactivatingValidators::<T>::remove(&account_id);

            Self::deposit_event(Event::<T>::ValidatorDeregistered { validator_id: account_id });

            weight = weight.saturating_add(operation_weight);
        }

        weight
    }

    fn cleanup_old_processed_transactions(
        current_block: BlockNumberFor<T>,
        max_weight: Weight,
    ) -> Weight {
        // Keep processed transactions for 48 hours (28800 blocks at 6s/block)
        // This prevents duplicate processing if T1 callbacks are delayed
        const RETENTION_PERIOD: u32 = 28800;
        let cutoff_block = current_block.saturating_sub(RETENTION_PERIOD.into());

        let mut weight = T::DbWeight::get().reads(1);

        if weight.ref_time() >= max_weight.ref_time() {
            return weight;
        }

        // BOUNDED: Limit both iterations AND results to prevent unbounded storage reads
        let mut to_remove = Vec::with_capacity(MAX_PROCESSED_TX_CLEANUPS_PER_BLOCK);
        let mut iterations = 0u32;
        const MAX_ITERATIONS: u32 = 200; // Safety limit: max storage reads per call

        for (tx_id, processed_at) in ProcessedTransactions::<T>::iter() {
            iterations += 1;

            // Safety: Stop if we've iterated too many times
            if iterations >= MAX_ITERATIONS {
                if to_remove.is_empty() {
                    log::warn!(
                        "⚠️  Reached iteration limit in cleanup_old_processed_transactions \
                        without finding any old transactions"
                    );
                }
                break;
            }

            // Check if this entry is old enough to remove
            if processed_at < cutoff_block {
                to_remove.push(tx_id);
                // Stop if we have enough results
                if to_remove.len() >= MAX_PROCESSED_TX_CLEANUPS_PER_BLOCK {
                    break;
                }
            }
        }

        // Account for actual iterations, not just results
        weight = weight.saturating_add(T::DbWeight::get().reads(iterations as u64));

        for tx_id in to_remove {
            let operation_weight = T::DbWeight::get().writes(1);
            if weight.saturating_add(operation_weight).ref_time() > max_weight.ref_time() {
                break;
            }

            ProcessedTransactions::<T>::remove(tx_id);
            weight = weight.saturating_add(operation_weight);
        }

        weight
    }

    fn start_activation_for_registered_validator(
        registered_validator: &T::AccountId,
        tx_id: EthereumId,
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
        !active_validators.iter().any(|v| &v.account_id == deregistered_validator)
            && !disabled_validators.iter().any(|v| v == deregistered_validator)
    }

    // Legacy remove_deregistered_validator has been removed - use new deregistration flow instead

    fn deregistration_state_is_active(status: ValidatorsActionStatus) -> bool {
        matches!(
            status,
            ValidatorsActionStatus::AwaitingConfirmation | ValidatorsActionStatus::Confirmed
        )
    }

    fn has_active_slash(validator_account_id: &T::AccountId) -> bool {
        <ValidatorActions<T>>::iter_prefix_values(validator_account_id).any(
            |validators_action_data| {
                validators_action_data.action_type == ValidatorsActionType::Slashed
                    && Self::deregistration_state_is_active(validators_action_data.status)
            },
        )
    }

    fn validate_validator_registration_request(
        account_id: &T::AccountId,
        eth_public_key: &ecdsa::Public,
        deposit: BalanceOf<T>,
    ) -> DispatchResult {
        // Check for conflicting operations
        Self::validate_no_conflicting_operations(account_id)?;

        let validator_account_ids =
            Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;
        ensure!(!validator_account_ids.is_empty(), Error::<T>::NoValidators);

        ensure!(!validator_account_ids.contains(account_id), Error::<T>::ValidatorAlreadyExists);

        ensure!(
            !<EthereumPublicKeys<T>>::contains_key(eth_public_key),
            Error::<T>::ValidatorEthKeyAlreadyExists
        );

        ensure!(
            validator_account_ids.len()
                < (<MaximumValidatorsBound as sp_core::TypedGet>::get() as usize),
            Error::<T>::MaximumValidatorsReached
        );

        // Validate staking preconditions
        Self::validate_staking_preconditions(account_id, deposit)?;

        Ok(())
    }

    fn validate_validator_deregistration_request(account_id: &T::AccountId) -> DispatchResult {
        // Check for conflicting operations
        Self::validate_no_conflicting_operations(account_id)?;

        let validator_account_ids =
            Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;

        ensure!(
            validator_account_ids.len() > T::MinimumValidatorCount::get() as usize,
            Error::<T>::MinimumValidatorsReached
        );

        ensure!(validator_account_ids.contains(account_id), Error::<T>::ValidatorNotFound);

        Ok(())
    }

    fn validate_no_conflicting_operations(account_id: &T::AccountId) -> DispatchResult {
        // Check for pending registration
        ensure!(
            !PendingValidatorRegistrations::<T>::contains_key(account_id),
            Error::<T>::RegistrationInProgress
        );

        // Check for pending deregistration
        ensure!(
            !PendingValidatorDeregistrations::<T>::contains_key(account_id),
            Error::<T>::DeregistrationInProgress
        );

        // Check for deactivation in progress
        ensure!(
            !DeactivatingValidators::<T>::contains_key(account_id),
            Error::<T>::ValidatorDeactivating
        );

        // Check for pending activation
        let has_pending_activation =
            ValidatorActions::<T>::iter_prefix_values(account_id).any(|action| {
                action.action_type == ValidatorsActionType::Activation
                    && Self::deregistration_state_is_active(action.status)
            });
        ensure!(!has_pending_activation, Error::<T>::ActivationInProgress);

        Ok(())
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
                "💔 Unable to find staking candidate info for collator: {:?}",
                action_account_id
            );
            return Err(());
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
            return Err(());
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
        tx_id: EthereumId,
        succeeded: bool,
    ) -> DispatchResult {
        let pending_data = PendingValidatorRegistrations::<T>::get(&account_id)
            .ok_or(Error::<T>::PendingRegistrationNotFound)?;

        if succeeded {
            Self::complete_validator_registration(&account_id, &pending_data)?;
        } else {
            Self::deposit_event(Event::<T>::ValidatorRegistrationFailed {
                validator_id: account_id.clone(),
                tx_id,
            });
        }

        frame_support::storage::transactional::with_transaction(|| {
            PendingValidatorRegistrations::<T>::remove(&account_id);
            PendingValidatorTransactions::<T>::remove(tx_id);

            frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
        })?;

        Ok(())
    }

    /// Handle the result of a validator deregistration request sent to T1
    fn handle_validator_deregistration_result(
        account_id: T::AccountId,
        tx_id: EthereumId,
        succeeded: bool,
    ) -> DispatchResult {
        let _pending_data = PendingValidatorDeregistrations::<T>::get(&account_id)
            .ok_or(Error::<T>::PendingDeregistrationNotFound)?;

        if succeeded {
            // Try to complete deregistration
            match Self::complete_validator_deregistration(&account_id) {
                Ok(_) => {
                    // Only cleanup pending state on success
                    frame_support::storage::transactional::with_transaction(|| {
                        PendingValidatorDeregistrations::<T>::remove(&account_id);
                        PendingValidatorTransactions::<T>::remove(tx_id);

                        frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(
                            (),
                        ))
                    })?;
                },
                Err(e) => {
                    // Emit special event indicating completion failure but T1 success
                    Self::deposit_event(Event::<T>::ValidatorDeregistrationCompletionFailed {
                        validator_id: account_id.clone(),
                        tx_id,
                    });

                    // DON'T remove pending state - allow force_complete_deregistration to retry
                    return Err(e);
                },
            }
        } else {
            // T1 failed - cleanup pending state
            frame_support::storage::transactional::with_transaction(|| {
                PendingValidatorDeregistrations::<T>::remove(&account_id);
                PendingValidatorTransactions::<T>::remove(tx_id);

                frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
            })?;

            Self::deposit_event(Event::<T>::ValidatorDeregistrationFailed {
                validator_id: account_id.clone(),
                tx_id,
            });
        }

        Ok(())
    }

    /// Store a pending validator registration request
    fn store_pending_validator_registration(
        account_id: &T::AccountId,
        eth_public_key: &ecdsa::Public,
        deposit: Option<BalanceOf<T>>,
        tx_id: EthereumId,
    ) -> DispatchResult {
        let current_block = <frame_system::Pallet<T>>::block_number();
        let pending_data = PendingValidatorRegistrationData {
            account_id: account_id.clone(),
            eth_public_key: *eth_public_key,
            tx_id,
            timestamp: current_block,
            deposit,
        };

        frame_support::storage::transactional::with_transaction(|| {
            PendingValidatorRegistrations::<T>::insert(account_id, pending_data);
            PendingValidatorTransactions::<T>::insert(
                tx_id,
                (account_id.clone(), PendingValidatorOperationType::Registration),
            );

            frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
        })?;

        Ok(())
    }

    /// Store a pending validator deregistration request
    fn store_pending_validator_deregistration(
        account_id: &T::AccountId,
        tx_id: EthereumId,
        reason: ValidatorDeregistrationReason,
    ) -> DispatchResult {
        let current_block = <frame_system::Pallet<T>>::block_number();
        let pending_data =
            PendingValidatorDeregistrationData { tx_id, timestamp: current_block, reason };

        // Atomic: both insertions succeed or both fail
        frame_support::storage::transactional::with_transaction(|| {
            PendingValidatorDeregistrations::<T>::insert(account_id, pending_data);
            PendingValidatorTransactions::<T>::insert(
                tx_id,
                (account_id.clone(), PendingValidatorOperationType::Deregistration),
            );

            frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
        })?;

        Ok(())
    }

    /// Send validator registration request to T1
    fn send_validator_registration_to_t1(
        validator_account_id: &T::AccountId,
        validator_eth_public_key: &ecdsa::Public,
    ) -> Result<EthereumId, DispatchError> {
        let decompressed_eth_public_key = decompress_eth_public_key(*validator_eth_public_key)
            .map_err(|_| Error::<T>::InvalidPublicKey)?;

        let validator_id_bytes =
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(validator_account_id);

        let function_name = BridgeContractMethod::AddAuthor.name_as_bytes();
        let params = vec![
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
            (b"bytes32".to_vec(), validator_id_bytes.to_vec()),
        ];

        <T as pallet::Config>::BridgeInterface::publish(function_name, &params, PALLET_ID.to_vec())
            .map_err(|_| Error::<T>::BridgePublishFailed.into())
    }

    /// Send validator deregistration request to T1
    fn send_validator_deregistration_to_t1(
        validator_account_id: &T::AccountId,
    ) -> Result<EthereumId, DispatchError> {
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

        <T as pallet::Config>::BridgeInterface::publish(function_name, &params, PALLET_ID.to_vec())
            .map_err(|_| Error::<T>::BridgePublishFailed.into())
    }

    /// Complete validator registration after T1 confirmation
    fn complete_validator_registration(
        account_id: &T::AccountId,
        pending_data: &PendingValidatorRegistrationData<
            T::AccountId,
            BlockNumberFor<T>,
            BalanceOf<T>,
        >,
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
                Err(e) => {
                    Self::deposit_event(Event::<T>::StakingOperationFailed {
                        validator_id: account_id.clone(),
                    });
                    return Err(e.error);
                },
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

        // Start activation process
        Self::start_activation_for_registered_validator(account_id, pending_data.tx_id);

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
            <T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(account_id.clone())),
            candidate_count,
        ) {
            Ok(_) => {},
            Err(e) => {
                return Err(e.error);
            },
        }

        // Mark validator as deactivating (not fully removed yet)
        DeactivatingValidators::<T>::insert(
            account_id,
            DeactivationData {
                scheduled_block: <frame_system::Pallet<T>>::block_number(),
                reason: ValidatorDeregistrationReason::Voluntary,
            },
        );

        // Remove ethereum key mapping
        Self::remove_ethereum_public_key_if_required(account_id);

        // Emit scheduled event (final removal happens in on_initialize)
        Self::deposit_event(Event::<T>::ValidatorDeregistrationScheduled {
            validator_id: account_id.clone(),
        });

        Ok(())
    }

    /// Get pending registration for an account
    pub fn get_pending_registration(
        account_id: &T::AccountId,
    ) -> Option<PendingValidatorRegistrationData<T::AccountId, BlockNumberFor<T>, BalanceOf<T>>>
    {
        PendingValidatorRegistrations::<T>::get(account_id)
    }

    /// Get pending deregistration for an account
    pub fn get_pending_deregistration(
        account_id: &T::AccountId,
    ) -> Option<PendingValidatorDeregistrationData<BlockNumberFor<T>>> {
        PendingValidatorDeregistrations::<T>::get(account_id)
    }

    /// Get all pending operations
    pub fn get_all_pending_operations() -> Vec<(T::AccountId, PendingValidatorOperationType)> {
        let mut operations = Vec::new();

        for (account_id, _) in PendingValidatorRegistrations::<T>::iter() {
            operations.push((account_id, PendingValidatorOperationType::Registration));
        }

        for (account_id, _) in PendingValidatorDeregistrations::<T>::iter() {
            operations.push((account_id, PendingValidatorOperationType::Deregistration));
        }

        operations
    }

    /// Get validator status
    pub fn get_validator_status(account_id: &T::AccountId) -> ValidatorStatus {
        if let Some(validator_account_ids) = Self::validator_account_ids() {
            if validator_account_ids.contains(account_id) {
                if DeactivatingValidators::<T>::contains_key(account_id) {
                    return ValidatorStatus::Deactivating;
                }
                return ValidatorStatus::Active;
            }
        }

        if PendingValidatorRegistrations::<T>::contains_key(account_id) {
            return ValidatorStatus::PendingRegistration;
        }

        if PendingValidatorDeregistrations::<T>::contains_key(account_id) {
            return ValidatorStatus::PendingDeregistration;
        }

        ValidatorStatus::NotValidator
    }

}

impl<T: Config> BridgeInterfaceNotification for Pallet<T> {
    fn process_result(tx_id: u32, caller_id: Vec<u8>, succeeded: bool) -> DispatchResult {
        if caller_id != PALLET_ID.to_vec() {
            return Ok(());
        }

        // Reentrancy protection: check if already processed
        if ProcessedTransactions::<T>::contains_key(tx_id) {
            return Ok(()); // Silently ignore duplicates
        }

        // Mark as processed FIRST
        ProcessedTransactions::<T>::insert(tx_id, <frame_system::Pallet<T>>::block_number());

        // Check if this is a pending operation transaction
        if let Some((account_id, operation_type)) = PendingValidatorTransactions::<T>::get(tx_id) {
            let result = match operation_type {
                PendingValidatorOperationType::Registration => {
                    Self::handle_validator_registration_result(account_id, tx_id, succeeded)
                },
                PendingValidatorOperationType::Deregistration => {
                    Self::handle_validator_deregistration_result(account_id, tx_id, succeeded)
                },
            };

            // If processing failed, remove from processed set to allow retry
            if result.is_err() {
                ProcessedTransactions::<T>::remove(tx_id);
            }

            result?;
        } else {
            // Unknown transaction - not from this pallet or already cleaned up
            Self::deposit_event(Event::<T>::UnknownTransactionCallback { tx_id });
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
                if validators_action_data.status == ValidatorsActionStatus::AwaitingConfirmation
                    && validators_action_data.action_type.is_deregistration()
                    && Self::validator_permanently_removed(
                        &active_validators,
                        &disabled_validators,
                        &action_account_id,
                    )
                {
                    Self::clean_up_collator_data(action_account_id, ingress_counter);
                } else if validators_action_data.status
                    == ValidatorsActionStatus::AwaitingConfirmation
                    && validators_action_data.action_type.is_activation()
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
                } else if validators_action_data.status == ValidatorsActionStatus::Confirmed
                    && validators_action_data.action_type.is_activation()
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
