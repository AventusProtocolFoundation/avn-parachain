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
use alloc::string::{String, ToString};

use frame_support::{dispatch::DispatchResult, ensure, log, traits::Get, transactional};
use frame_system::{self as system, ensure_none, offchain::SendTransactionTypes, RawOrigin};
use pallet_session::{self as session, Config as SessionConfig};
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{Convert, Member},
    transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidity},
    DispatchError,
};
use sp_std::prelude::*;

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_avn::{
    self as avn,
    vote::{
        approve_vote_validate_unsigned, end_voting_period_validate_unsigned, process_approve_vote,
        process_reject_vote, reject_vote_validate_unsigned, VotingSessionData,
        VotingSessionManager, APPROVE_VOTE, REJECT_VOTE,
    },
    AccountToBytesConverter, DisabledValidatorChecker, Enforcer, Error as avn_error,
    EthereumPublicKeyChecker, NewSessionHandler, ProcessedEventsChecker,
    ValidatorRegistrationNotifier,
};
use pallet_ethereum_transactions::ethereum_transaction::{
    ActivateCollatorData, DeregisterCollatorData, EthAbiHelper, EthTransactionType, TransactionId,
};
use sp_application_crypto::RuntimeAppPublic;
use sp_avn_common::{
    bounds::{MaximumValidatorsBound, VotingSessionIdBound},
    eth_key_actions::decompress_eth_public_key,
    event_types::Validator,
    safe_add_block_numbers, IngressCounter,
};
use sp_core::{bounded::BoundedVec, ecdsa, H512};

pub use pallet_parachain_staking::{self as parachain_staking, BalanceOf, PositiveImbalanceOf};
use pallet_session::historical::IdentificationTuple;
use sp_io::hashing::keccak_256;
use sp_staking::offence::ReportOffence;

pub use pallet::*;
pub mod vote;
use crate::vote::*;
pub mod confirmations;
use crate::confirmations::*;
pub mod offence;
use crate::offence::{
    create_and_report_validators_offence, ValidatorOffence, ValidatorOffenceType,
};

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{assert_ok, pallet_prelude::*};
    use frame_system::pallet_prelude::*;

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

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
        type VotingPeriod: Get<Self::BlockNumber>;
        /// A trait that allows converting between accountIds <-> public keys
        type AccountToBytesConvert: AccountToBytesConverter<Self::AccountId>;
        /// A trait that allows extra work to be done during validator registration
        type ValidatorRegistrationNotifier: ValidatorRegistrationNotifier<
            <Self as session::Config>::ValidatorId,
        >;
        ///  A type that gives the pallet the ability to report offences
        type ReportValidatorOffence: ReportOffence<
            Self::AccountId,
            IdentificationTuple<Self>,
            ValidatorOffence<IdentificationTuple<Self>>,
        >;

        /// Weight information for the extrinsics in this pallet.
        type WeightInfo: WeightInfo;
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
        VoteAdded {
            voter_id: T::AccountId,
            action_id: ActionId<T::AccountId>,
            approve: bool,
        },
        VotingEnded {
            action_id: ActionId<T::AccountId>,
            vote_approved: bool,
        },
        ValidatorActionConfirmed {
            action_id: ActionId<T::AccountId>,
        },
        ValidatorSlashed {
            action_id: ActionId<T::AccountId>,
        },
        OffenceReported {
            offence_type: ValidatorOffenceType,
            offenders: Vec<IdentificationTuple<T>>,
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
        ValidatorsActionData<T::AccountId>,
        OptionQuery,
        GetDefault,
    >;

    #[pallet::storage]
    #[pallet::getter(fn get_vote)]
    pub type VotesRepository<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        ActionId<T::AccountId>,
        VotingSessionData<T::AccountId, T::BlockNumber>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn get_pending_actions)]
    pub type PendingApprovals<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, IngressCounter, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_validator_by_eth_public_key)]
    pub type EthereumPublicKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, ecdsa::Public, T::AccountId>;

    #[pallet::storage]
    #[pallet::getter(fn get_ingress_counter)]
    pub type TotalIngresses<T: Config> = StorageValue<_, IngressCounter, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub validators: Vec<(T::AccountId, ecdsa::Public)>,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { validators: Vec::<(T::AccountId, ecdsa::Public)>::new() }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            log::debug!(
                "Validators Manager Genesis build entrypoint - total validators: {}",
                self.validators.len()
            );
            for (validator_account_id, eth_public_key) in &self.validators {
                assert_ok!(<ValidatorAccountIds<T>>::try_append(validator_account_id));
                <EthereumPublicKeys<T>>::insert(eth_public_key, validator_account_id);
            }
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Sudo function to add a collator.
        /// This will call the `join_candidates` method in the parachain_staking pallet.
        /// [transactional]: this makes `add_validator` behave like an ethereum transaction (atomic tx). No need to use VFWL.
        /// see here for more info: https://github.com/paritytech/substrate/issues/10806
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
            let validator_account_ids =
                Self::validator_account_ids().ok_or(Error::<T>::NoValidators)?;
            ensure!(validator_account_ids.len() > 0, Error::<T>::NoValidators);

            ensure!(
                !validator_account_ids.contains(&collator_account_id),
                Error::<T>::ValidatorAlreadyExists
            );
            ensure!(
                !<EthereumPublicKeys<T>>::contains_key(&collator_eth_public_key),
                Error::<T>::ValidatorEthKeyAlreadyExists
            );

            // This early check ensures a consistent pallet interface, regardless of the staking
            // pallet's configuration. The staking pallet uses a different bound for
            // collator candidates, which could result in its own error code.
            ensure!(
                ValidatorAccountIds::<T>::get().unwrap_or_default().len() <
                    (<MaximumValidatorsBound as sp_core::TypedGet>::get() as usize),
                Error::<T>::MaximumValidatorsReached
            );

            let candidate_count = parachain_staking::Pallet::<T>::candidate_pool().0.len() as u32;
            let bond = deposit
                .or_else(|| Some(parachain_staking::Pallet::<T>::min_collator_stake()))
                .expect("has default value");
            let register_as_candidate_weight = parachain_staking::Pallet::<T>::join_candidates(
                <T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(
                    collator_account_id.clone(),
                )),
                bond,
                candidate_count,
            )?;

            Self::register_validator(&collator_account_id, &collator_eth_public_key)?;

            <ValidatorAccountIds<T>>::try_append(collator_account_id.clone())
                .map_err(|_| Error::<T>::MaximumValidatorsReached)?;
            <EthereumPublicKeys<T>>::insert(collator_eth_public_key, collator_account_id);

            // TODO: benchmark `register_validator` and add to the weight
            return Ok(Some(
                register_as_candidate_weight
                    .actual_weight
                    .or_else(|| Some(Weight::zero()))
                    .expect("Has default value")
                    .saturating_add(T::DbWeight::get().reads_writes(0, 2))
                    .saturating_add(Weight::from_ref_time(40_000_000)),
            )
            .into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_validator(MAX_VALIDATOR_ACCOUNT_IDS))]
        #[transactional]
        pub fn remove_validator(
            origin: OriginFor<T>,
            collator_account_id: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            let _ = ensure_root(origin)?;

            // remove collator from parachain_staking pallet
            let candidate_count = parachain_staking::Pallet::<T>::candidate_pool().0.len() as u32;
            let resign_as_candidate_weight =
                parachain_staking::Pallet::<T>::schedule_leave_candidates(
                    <T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(
                        collator_account_id.clone(),
                    )),
                    candidate_count,
                )?;

            // TODO [TYPE: security][PRI: low][CRITICAL][JIRA: 66]: ensure that we have
            // authorization from the whole of T2? This is part of the package to
            // implement validator removals, slashing and the economics around that
            Self::remove_deregistered_validator(&collator_account_id)?;

            Self::deposit_event(Event::<T>::ValidatorDeregistered {
                validator_id: collator_account_id,
            });

            // TODO: benchmark `remove_deregistered_validator` and add to the weight
            return Ok(Some(
                resign_as_candidate_weight
                    .actual_weight
                    .or_else(|| Some(Weight::zero()))
                    .expect("Has default value")
                    .saturating_add(Weight::from_ref_time(40_000_000)),
            )
            .into())
        }

        #[pallet::call_index(2)]
        #[pallet::weight( <T as Config>::WeightInfo::approve_action_with_end_voting(MAX_VALIDATOR_ACCOUNT_IDS))]
        pub fn approve_validator_action(
            origin: OriginFor<T>,
            action_id: ActionId<T::AccountId>,
            validator: Validator<T::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;

            let voting_session = Self::get_voting_session(&action_id);

            process_approve_vote::<T>(&voting_session, validator.account_id.clone())?;

            Self::deposit_event(Event::<T>::VoteAdded {
                voter_id: validator.account_id,
                action_id,
                approve: APPROVE_VOTE,
            });

            // TODO [TYPE: weightInfo][PRI: medium]: Refund unused weights
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight( <T as Config>::WeightInfo::reject_action_with_end_voting(MAX_VALIDATOR_ACCOUNT_IDS))]
        pub fn reject_validator_action(
            origin: OriginFor<T>,
            action_id: ActionId<T::AccountId>,
            validator: Validator<T::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            let voting_session = Self::get_voting_session(&action_id);

            process_reject_vote::<T>(&voting_session, validator.account_id.clone())?;

            Self::deposit_event(Event::<T>::VoteAdded {
                voter_id: validator.account_id,
                action_id,
                approve: REJECT_VOTE,
            });

            // TODO [TYPE: weightInfo][PRI: medium]: Refund unused weights
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight( <T as Config>::WeightInfo::end_voting_period_with_rejected_valid_actions(MAX_OFFENDERS)
            .max(<T as Config>::WeightInfo::end_voting_period_with_approved_invalid_actions(MAX_OFFENDERS)))]
        pub fn end_voting_period(
            origin: OriginFor<T>,
            action_id: ActionId<T::AccountId>,
            validator: Validator<T::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            //Event is deposited in end_voting because this function can get called from
            // `approve_validator_action` or `reject_validator_action`
            Self::end_voting(validator.account_id, &action_id)?;

            // TODO [TYPE: weightInfo][PRI: medium]: Refund unused weights
            Ok(())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn offchain_worker(block_number: T::BlockNumber) {
            let setup_result = AVN::<T>::pre_run_setup(block_number, NAME.to_vec());
            if let Err(e) = setup_result {
                match e {
                    _ if e == DispatchError::from(avn_error::<T>::OffchainWorkerAlreadyRun) => {
                        ();
                    },
                    _ => {
                        log::error!("💔 Unable to run offchain worker: {:?}", e);
                    },
                };

                return
            }
            let this_validator = setup_result.expect("We have a validator");

            cast_votes_if_required::<T>(&this_validator);
            end_voting_if_required::<T>(block_number, &this_validator);
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            if let Call::end_voting_period {
                action_id: deregistered_validator,
                validator,
                signature,
            } = call
            {
                let voting_session = Self::get_voting_session(deregistered_validator);
                return end_voting_period_validate_unsigned::<T>(
                    &voting_session,
                    validator,
                    signature,
                )
            } else if let Call::approve_validator_action { action_id, validator, signature } = call
            {
                if !<ValidatorActions<T>>::contains_key(
                    &action_id.action_account_id,
                    action_id.ingress_counter,
                ) {
                    return InvalidTransaction::Custom(ERROR_CODE_INVALID_DEREGISTERED_VALIDATOR)
                        .into()
                }

                let voting_session = Self::get_voting_session(action_id);

                return approve_vote_validate_unsigned::<T>(&voting_session, validator, signature)
            } else if let Call::reject_validator_action {
                action_id: deregistered_validator,
                validator,
                signature,
            } = call
            {
                let voting_session = Self::get_voting_session(deregistered_validator);
                return reject_vote_validate_unsigned::<T>(&voting_session, validator, signature)
            } else {
                return InvalidTransaction::Call.into()
            }
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
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ValidatorsActionData<AccountId: Member> {
    pub status: ValidatorsActionStatus,
    pub primary_validator: AccountId,
    pub eth_transaction_id: TransactionId,
    pub action_type: ValidatorsActionType,
    pub reserved_eth_transaction: EthTransactionType,
}

impl<AccountId: Member> ValidatorsActionData<AccountId> {
    fn new(
        status: ValidatorsActionStatus,
        primary_validator: AccountId,
        eth_transaction_id: TransactionId,
        action_type: ValidatorsActionType,
        reserved_eth_transaction: EthTransactionType,
    ) -> Self {
        return ValidatorsActionData::<AccountId> {
            status,
            primary_validator,
            eth_transaction_id,
            action_type,
            reserved_eth_transaction,
        }
    }
}

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/tests_voting_deregistration.rs"]
mod tests_voting_deregistration;

#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;

mod benchmarking;

pub mod default_weights;
pub use default_weights::WeightInfo;

// used in benchmarks and weights calculation only
const MAX_VALIDATOR_ACCOUNT_IDS: u32 = 10;
const MAX_OFFENDERS: u32 = 2;

// TODO [TYPE: review][PRI: medium]: if needed, make this the default value to a configurable
// option. See MinimumValidatorCount in Staking pallet as a reference
const DEFAULT_MINIMUM_VALIDATORS_COUNT: usize = 2;

const NAME: &'static [u8; 17] = b"validatorsManager";

// Error codes returned by validate unsigned methods
const ERROR_CODE_INVALID_DEREGISTERED_VALIDATOR: u8 = 10;

pub type AVN<T> = avn::Pallet<T>;

impl<T: Config> Pallet<T> {
    pub fn get_voting_session(
        action_id: &ActionId<T::AccountId>,
    ) -> Box<dyn VotingSessionManager<T::AccountId, T::BlockNumber>> {
        return Box::new(ValidatorManagementVotingSession::<T>::new(action_id))
            as Box<dyn VotingSessionManager<T::AccountId, T::BlockNumber>>
    }

    pub fn abi_encode_collator_action_data(
        action_id: &ActionId<T::AccountId>,
    ) -> Result<String, DispatchError> {
        let validators_action_data = Self::try_get_validators_action_data(action_id)?;

        let action_parameters_concat_hash = match validators_action_data.reserved_eth_transaction {
            EthTransactionType::ActivateCollator(ref d) => concat_and_hash_activation_data(d),
            EthTransactionType::DeregisterCollator(ref d) => concat_and_hash_deregistration_data(d),
            _ => Err(Error::<T>::ErrorGeneratingEthDescription)?,
        };

        let sender = <T as pallet::Config>::AccountToBytesConvert::into_bytes(
            &validators_action_data.primary_validator,
        );

        // Now treat this as an bytes32 parameter and generate signing abi.
        let confirmation_data = EthAbiHelper::generate_ethereum_abi_data_for_signature_request(
            &action_parameters_concat_hash,
            validators_action_data.eth_transaction_id,
            &sender,
        );

        let msg_hash = keccak_256(&confirmation_data);

        log::info!(
            "📄 Data used for abi encode: (hex-encoded hash: {:?}, tx_id: {:?}, hex-encoded sender: {:?}). Output: {:?}",
            hex::encode(action_parameters_concat_hash),
            validators_action_data.eth_transaction_id,
            hex::encode(&sender),
            &msg_hash
        );

        Ok(hex::encode(msg_hash))
    }

    fn try_get_validators_action_data(
        action_id: &ActionId<T::AccountId>,
    ) -> Result<ValidatorsActionData<T::AccountId>, Error<T>> {
        if <ValidatorActions<T>>::contains_key(
            &action_id.action_account_id,
            action_id.ingress_counter,
        ) {
            return <ValidatorActions<T>>::get(
                &action_id.action_account_id,
                action_id.ingress_counter,
            )
            .ok_or(Error::<T>::ValidatorsActionDataNotFound)
        }

        Err(Error::<T>::ValidatorsActionDataNotFound)?
    }

    fn end_voting(sender: T::AccountId, action_id: &ActionId<T::AccountId>) -> DispatchResult {
        let voting_session = Self::get_voting_session(&action_id);

        ensure!(voting_session.is_valid(), Error::<T>::VotingSessionIsNotValid);

        let vote = voting_session.state()?;

        ensure!(Self::can_end_vote(&vote), Error::<T>::ErrorEndingVotingPeriod);

        let vote_is_approved = vote.is_approved();

        if vote_is_approved {
            create_and_report_validators_offence::<T>(
                &sender,
                &vote.nays,
                ValidatorOffenceType::RejectedValidAction,
            );

            <ValidatorActions<T>>::mutate(
                &action_id.action_account_id,
                action_id.ingress_counter,
                |validators_action_data_maybe| {
                    if let Some(validators_action_data) = validators_action_data_maybe {
                        validators_action_data.status = ValidatorsActionStatus::Actioned
                    }
                },
            );
        } else {
            // We didn't get enough votes to approve this deregistration
            create_and_report_validators_offence::<T>(
                &sender,
                &vote.ayes,
                ValidatorOffenceType::ApprovedInvalidAction,
            );
        }

        <PendingApprovals<T>>::remove(&action_id.action_account_id);

        Self::deposit_event(Event::<T>::VotingEnded {
            action_id: action_id.clone(),
            vote_approved: vote_is_approved,
        });

        Ok(())
    }

    fn can_end_vote(vote: &VotingSessionData<T::AccountId, T::BlockNumber>) -> bool {
        return vote.has_outcome() ||
            <system::Pallet<T>>::block_number() >= vote.end_of_voting_period
    }

    /// Helper function to help us fail early if any of the data we need is not available for the
    /// registration & activation
    fn prepare_registration_data(
        collator_eth_public_key: &ecdsa::Public,
        collator_id: &T::AccountId,
    ) -> Result<
        (
            <T as pallet_session::Config>::ValidatorId,
            T::AccountId,
            EthTransactionType,
            TransactionId,
        ),
        DispatchError,
    > {
        let new_collator_id = <T as SessionConfig>::ValidatorIdOf::convert(collator_id.clone())
            .ok_or(Error::<T>::ErrorConvertingAccountIdToValidatorId)?;
        let decompressed_collator_eth_public_key =
            decompress_eth_public_key(*collator_eth_public_key)
                .map_err(|_| Error::<T>::InvalidPublicKey)?;
        let eth_tx_sender =
            AVN::<T>::calculate_primary_validator(<system::Pallet<T>>::block_number())
                .map_err(|_| Error::<T>::ErrorCalculatingPrimaryValidator)?;
        let eth_transaction_type = EthTransactionType::ActivateCollator(ActivateCollatorData::new(
            decompressed_collator_eth_public_key,
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(&collator_id),
        ));
        let tx_id = 0;

        Ok((new_collator_id, eth_tx_sender, eth_transaction_type, tx_id))
    }

    fn start_activation_for_registered_validator(
        registered_validator: &T::AccountId,
        eth_tx_sender: T::AccountId,
        eth_transaction_type: EthTransactionType,
        tx_id: TransactionId,
    ) {
        let ingress_counter = Self::get_ingress_counter() + 1;

        TotalIngresses::<T>::put(ingress_counter);
        <ValidatorActions<T>>::insert(
            registered_validator,
            ingress_counter,
            ValidatorsActionData::new(
                ValidatorsActionStatus::AwaitingConfirmation,
                eth_tx_sender,
                tx_id,
                ValidatorsActionType::Activation,
                eth_transaction_type,
            ),
        );
    }

    fn register_validator(
        collator_account_id: &T::AccountId,
        collator_eth_public_key: &ecdsa::Public,
    ) -> DispatchResult {
        let (new_validator_id, eth_tx_sender, eth_transaction_type, tx_id) =
            Self::prepare_registration_data(collator_eth_public_key, collator_account_id)?;

        Self::start_activation_for_registered_validator(
            collator_account_id,
            eth_tx_sender,
            eth_transaction_type,
            tx_id,
        );
        T::ValidatorRegistrationNotifier::on_validator_registration(&new_validator_id);

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
        eth_transaction_type: EthTransactionType,
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

        // TODO: decide if this is the best way to handle this
        let eth_tx_sender =
            AVN::<T>::calculate_primary_validator(<system::Pallet<T>>::block_number())
                .map_err(|_| Error::<T>::ErrorCalculatingPrimaryValidator)?;

        let tx_id = 0;

        TotalIngresses::<T>::put(ingress_counter);
        <ValidatorActions<T>>::insert(
            validator_id,
            ingress_counter,
            ValidatorsActionData::new(
                ValidatorsActionStatus::AwaitingConfirmation,
                eth_tx_sender,
                tx_id,
                action_type,
                eth_transaction_type,
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
        let decompressed_eth_public_key = decompress_eth_public_key(t1_eth_public_key)
            .map_err(|_| Error::<T>::InvalidPublicKey)?;
        let candidate_tx = EthTransactionType::DeregisterCollator(DeregisterCollatorData::new(
            decompressed_eth_public_key,
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(resigned_validator),
        ));
        let ingress_counter = Self::get_ingress_counter() + 1;
        return Self::remove(
            resigned_validator,
            ingress_counter,
            ValidatorsActionType::Resignation,
            candidate_tx,
        )
    }

    fn can_setup_voting_to_activate_validator(
        validators_action_data: &ValidatorsActionData<T::AccountId>,
        action_account_id: &T::AccountId,
        active_validators: &Vec<Validator<T::AuthorityId, T::AccountId>>,
    ) -> bool {
        return validators_action_data.status == ValidatorsActionStatus::AwaitingConfirmation &&
            validators_action_data.action_type == ValidatorsActionType::Activation &&
            active_validators.iter().any(|v| &v.account_id == action_account_id)
    }

    fn setup_voting_to_activate_validator(
        ingress_counter: IngressCounter,
        validator_to_activate: &T::AccountId,
        quorum: u32,
        voting_period_end: T::BlockNumber,
    ) {
        <ValidatorActions<T>>::mutate(
            &validator_to_activate,
            ingress_counter,
            |validators_action_data_maybe| {
                if let Some(validators_action_data) = validators_action_data_maybe {
                    validators_action_data.status = ValidatorsActionStatus::Confirmed
                }
            },
        );

        <PendingApprovals<T>>::insert(&validator_to_activate, ingress_counter);

        let action_id = ActionId::new(validator_to_activate.clone(), ingress_counter);
        <VotesRepository<T>>::insert(
            action_id.clone(),
            VotingSessionData::new(
                action_id.session_id(),
                quorum,
                voting_period_end,
                <system::Pallet<T>>::block_number(),
            ),
        );
        Self::deposit_event(Event::<T>::ValidatorActivationStarted {
            validator_id: validator_to_activate.clone(),
        });
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

    fn clean_up_collator_data(
        action_account_id: T::AccountId,
        ingress_counter: IngressCounter,
        quorum: u32,
        voting_period_end: T::BlockNumber,
    ) {
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

            <PendingApprovals<T>>::insert(&action_account_id, ingress_counter);

            Self::remove_ethereum_public_key_if_required(&action_account_id);

            let action_id = ActionId::new(action_account_id, ingress_counter);
            <VotesRepository<T>>::insert(
                action_id.clone(),
                VotingSessionData::new(
                    action_id.session_id(),
                    quorum,
                    voting_period_end,
                    <system::Pallet<T>>::block_number(),
                ),
            );

            Self::deposit_event(Event::<T>::ValidatorActionConfirmed { action_id });
        }
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
    fn session_id(&self) -> BoundedVec<u8, VotingSessionIdBound> {
        BoundedVec::truncate_from(self.encode())
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
            let quorum = AVN::<T>::quorum();
            let voting_period_end =
                safe_add_block_numbers(<system::Pallet<T>>::block_number(), T::VotingPeriod::get());

            if let Err(e) = voting_period_end {
                log::error!("💔 Unable to calculate voting period end: {:?}", e);
                return
            }
            let voting_period_end = voting_period_end.expect("already checked");

            for (action_account_id, ingress_counter, validators_action_data) in
                <ValidatorActions<T>>::iter()
            {
                // TODO: Investigate if can_setup_voting_to_activate_validator can be used for
                // deregistration as well
                if validators_action_data.status == ValidatorsActionStatus::AwaitingConfirmation &&
                    validators_action_data.action_type.is_deregistration() &&
                    Self::validator_permanently_removed(
                        &active_validators,
                        &disabled_validators,
                        &action_account_id,
                    )
                {
                    Self::clean_up_collator_data(
                        action_account_id,
                        ingress_counter,
                        quorum,
                        voting_period_end,
                    );
                } else if Self::can_setup_voting_to_activate_validator(
                    &validators_action_data,
                    &action_account_id,
                    active_validators,
                ) {
                    Self::setup_voting_to_activate_validator(
                        ingress_counter,
                        &action_account_id,
                        AVN::<T>::quorum(),
                        voting_period_end,
                    );
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
