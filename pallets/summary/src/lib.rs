#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use codec::{Decode, Encode, MaxEncodedLen};
use sp_avn_common::{
    bounds::VotingSessionIdBound,
    event_types::Validator,
    ocw_lock::{self as OcwLock},
    safe_add_block_numbers, safe_sub_block_numbers, IngressCounter,
};
use sp_io::hashing::keccak_256;
use sp_runtime::{
    scale_info::TypeInfo,
    traits::AtLeast32Bit,
    transaction_validity::{
        InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
        ValidTransaction,
    },
    BoundedVec, DispatchError,
};
use sp_std::prelude::*;

use avn::OnBridgePublisherResult;
use core::convert::TryInto;
use frame_support::{dispatch::DispatchResult, ensure, log, traits::Get, weights::Weight};
use frame_system::{
    self as system, ensure_none, ensure_root,
    offchain::{SendTransactionTypes, SubmitTransaction},
};
pub use pallet::*;
use pallet_avn::{
    self as avn,
    vote::{
        approve_vote_validate_unsigned, end_voting_period_validate_unsigned, process_approve_vote,
        process_reject_vote, reject_vote_validate_unsigned, VotingSessionData,
        VotingSessionManager,
    },
    Error as avn_error,
};
use pallet_session::historical::IdentificationTuple;
use sp_application_crypto::RuntimeAppPublic;
use sp_core::{ecdsa, H256};
use sp_staking::offence::ReportOffence;
pub type EthereumTransactionId = u32;

pub mod offence;
use crate::offence::{create_and_report_summary_offence, SummaryOffence, SummaryOffenceType};

const NAME: &'static [u8; 7] = b"summary";
const UPDATE_BLOCK_NUMBER_CONTEXT: &'static [u8] = b"update_last_processed_block_number";
const ADVANCE_SLOT_CONTEXT: &'static [u8] = b"advance_slot";

// Error codes returned by validate unsigned methods
const ERROR_CODE_VALIDATOR_IS_NOT_PRIMARY: u8 = 10;
const ERROR_CODE_INVALID_ROOT_DATA: u8 = 20;
const ERROR_CODE_INVALID_ROOT_RANGE: u8 = 30;

// This value is used only when generating a signature for an empty root.
// Empty roots shouldn't be submitted to ethereum-transactions so we can use any value we want.
const EMPTY_ROOT_TRANSACTION_ID: EthereumTransactionId = 0;

// used in benchmarks and weights calculation only
const MAX_VALIDATOR_ACCOUNT_IDS: u32 = 10;
const MAX_OFFENDERS: u32 = 2; // maximum of offenders need to be less one third of minimum validators so the benchmark won't panic
const MAX_NUMBER_OF_ROOT_DATA_PER_RANGE: u32 = 2;

const MIN_SCHEDULE_PERIOD: u32 = 120; // 6 MINUTES
const DEFAULT_SCHEDULE_PERIOD: u32 = 28800; // 1 DAY
const MIN_VOTING_PERIOD: u32 = 100; // 5 MINUTES
const MAX_VOTING_PERIOD: u32 = 28800; // 1 DAY
const DEFAULT_VOTING_PERIOD: u32 = 600; // 30 MINUTES

pub mod vote;
use crate::vote::*;

pub mod util;

pub mod challenge;
use crate::challenge::*;

use pallet_avn::BridgePublisher;

mod benchmarking;
pub mod default_weights;
pub use default_weights::WeightInfo;

pub type AVN<T> = avn::Pallet<T>;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, Blake2_128Concat};
    use frame_system::pallet_prelude::*;

    // Public interface of this pallet
    #[pallet::config]
    pub trait Config:
        SendTransactionTypes<Call<Self>>
        + frame_system::Config
        + avn::Config
        + pallet_session::historical::Config
    {
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as frame_system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// A period (in block number) to detect when a validator failed to advance the current slot
        /// number
        type AdvanceSlotGracePeriod: Get<Self::BlockNumber>;

        /// Minimum age of block (in block number) to include in a tree.
        /// This will give grandpa a chance to finalise the blocks
        type MinBlockAge: Get<Self::BlockNumber>;

        // type CandidateTransactionSubmitter: CandidateTransactionSubmitter<Self::AccountId>;

        type AccountToBytesConvert: pallet_avn::AccountToBytesConverter<Self::AccountId>;

        ///  A type that gives the pallet the ability to report offences
        type ReportSummaryOffence: ReportOffence<
            Self::AccountId,
            IdentificationTuple<Self>,
            SummaryOffence<IdentificationTuple<Self>>,
        >;

        /// Weight information for the extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        type BridgePublisher: avn::BridgePublisher;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    /// This attribute generate the function `deposit_event` to deposit one of this pallet event,
    /// it is optional, it is also possible to provide a custom implementation.
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Schedule period and voting period are updated
        SchedulePeriodAndVotingPeriodUpdated {
            schedule_period: T::BlockNumber,
            voting_period: T::BlockNumber,
        },
        /// Root hash of summary between from block number and to block number is calculated by a
        /// validator
        SummaryCalculated {
            from: T::BlockNumber,
            to: T::BlockNumber,
            root_hash: H256,
            submitter: T::AccountId,
        },
        /// Vote by a voter for a root id is added
        VoteAdded { voter: T::AccountId, root_id: RootId<T::BlockNumber>, agree_vote: bool },
        /// Voting for the root id is finished, true means the root is approved
        VotingEnded { root_id: RootId<T::BlockNumber>, vote_approved: bool },
        /// A summary offence by a list of offenders is reported
        SummaryOffenceReported {
            offence_type: SummaryOffenceType,
            offenders: Vec<IdentificationTuple<T>>,
        },
        /// A new slot between a range of blocks for a validator is advanced by an account
        SlotAdvanced {
            advanced_by: T::AccountId,
            new_slot: T::BlockNumber,
            slot_validator: T::AccountId,
            slot_end: T::BlockNumber,
        },
        /// A summary created by a challengee is challenged by a challenger for a reason
        ChallengeAdded {
            challenge_reason: SummaryChallengeReason,
            challenger: T::AccountId,
            challengee: T::AccountId,
        },
        /// An offence about a summary not be published by a challengee is reported
        SummaryNotPublishedOffence {
            challengee: T::AccountId,
            /* slot number where no summary was published */
            void_slot: T::BlockNumber,
            /* slot where a block was last published */
            last_published: T::BlockNumber,
            /* block number for end of the void slot */
            end_vote: T::BlockNumber,
        },
        /// A summary root validated
        SummaryRootValidated {
            root_hash: H256,
            ingress_counter: IngressCounter,
            block_range: RootRange<T::BlockNumber>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        Overflow,
        ErrorCalculatingChosenValidator,
        ErrorConvertingBlockNumber,
        ErrorGettingSummaryDataFromService,
        InvalidSummaryRange,
        ErrorSubmittingTransaction,
        InvalidKey,
        ErrorSigning,
        InvalidHexString,
        InvalidUTF8Bytes,
        InvalidRootHashLength,
        SummaryPendingOrApproved,
        RootHasAlreadyBeenRegisteredForVoting,
        InvalidRoot,
        DuplicateVote,
        ErrorEndingVotingPeriod,
        ErrorSubmitCandidateTxnToTier1,
        VotingSessionIsNotValid,
        ErrorRecoveringPublicKeyFromSignature,
        ECDSASignatureNotValid,
        RootDataNotFound,
        InvalidChallenge,
        WrongValidator,
        GracePeriodElapsed,
        TooEarlyToAdvance,
        InvalidIngressCounter,
        SchedulePeriodIsTooShort,
        VotingPeriodIsTooShort,
        VotingPeriodIsTooLong,
        VotingPeriodIsEqualOrLongerThanSchedulePeriod,
        CurrentSlotValidatorNotFound,
        ErrorPublishingSummary,
    }

    // Note for SYS-152 (see notes in fn end_voting)):
    // A new instance of root_range should only be accepted into the system
    // (record_summary_calculation) if:
    // - there is no previous instance of that root_range in roots
    // - if there is any such an instance, it does not exist in PendingApprovals and it is not
    //   validated
    // It does not help to remove the root_range from Roots. If that were the case, we would lose
    // the information the root has already been processed and so cannot be submitted (ie voted
    // on) again.

    #[pallet::storage]
    #[pallet::getter(fn get_next_block_to_process)]
    pub type NextBlockToProcess<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn block_number_for_next_slot)]
    pub type NextSlotAtBlock<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn current_slot)]
    pub type CurrentSlot<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    // TODO: [STATE MIGRATION] - this storage item was changed from returning a default value to
    // returning an option
    #[pallet::storage]
    #[pallet::getter(fn slot_validator)]
    pub type CurrentSlotsValidator<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_summary_slot)]
    pub type SlotOfLastPublishedSummary<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    // TODO: [STATE MIGRATION] - this storage item was changed to make RootData.added_by an
    // Option<AccountID> instead of AccountId
    #[pallet::storage]
    pub type Roots<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        RootRange<T::BlockNumber>,
        Blake2_128Concat,
        IngressCounter,
        RootData<T::AccountId>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn get_vote )]
    pub type VotesRepository<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        RootId<T::BlockNumber>,
        VotingSessionData<T::AccountId, T::BlockNumber>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn get_pending_roots)]
    pub type PendingApproval<T: Config> =
        StorageMap<_, Blake2_128Concat, RootRange<T::BlockNumber>, IngressCounter, ValueQuery>;

    /// The total ingresses of roots
    #[pallet::storage]
    #[pallet::getter(fn get_ingress_counter)]
    pub type TotalIngresses<T: Config> = StorageValue<_, IngressCounter, ValueQuery>;

    /// A period (in block number) where summaries are calculated
    #[pallet::storage]
    #[pallet::getter(fn schedule_period)]
    pub type SchedulePeriod<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    /// A period (in block number) where validators are allowed to vote on the validity of a root
    /// hash
    #[pallet::storage]
    #[pallet::getter(fn voting_period)]
    pub type VotingPeriod<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub schedule_period: T::BlockNumber,
        pub voting_period: T::BlockNumber,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                schedule_period: T::BlockNumber::from(DEFAULT_SCHEDULE_PERIOD),
                voting_period: T::BlockNumber::from(DEFAULT_VOTING_PERIOD),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            let mut schedule_period_in_blocks = self.schedule_period;
            if schedule_period_in_blocks == 0u32.into() {
                schedule_period_in_blocks = DEFAULT_SCHEDULE_PERIOD.into();
            }
            assert!(
                Pallet::<T>::validate_schedule_period(schedule_period_in_blocks).is_ok(),
                "Schedule Period must be a valid value"
            );
            <NextSlotAtBlock<T>>::put(schedule_period_in_blocks);
            <SchedulePeriod<T>>::put(schedule_period_in_blocks);

            let mut voting_period_in_blocks = self.voting_period;
            if voting_period_in_blocks == 0u32.into() {
                voting_period_in_blocks = MIN_VOTING_PERIOD.into();
            }
            assert!(
                Pallet::<T>::validate_voting_period(
                    voting_period_in_blocks,
                    schedule_period_in_blocks
                )
                .is_ok(),
                "Voting Period must be a valid value"
            );
            <VotingPeriod<T>>::put(voting_period_in_blocks);

            let maybe_first_validator =
                AVN::<T>::validators().into_iter().map(|v| v.account_id).nth(0);
            assert!(maybe_first_validator.is_some(), "You must add validators to run the AvN");

            <CurrentSlotsValidator<T>>::put(
                maybe_first_validator.expect("Validator is checked for none"),
            );
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight( <T as pallet::Config>::WeightInfo::set_periods())]
        #[pallet::call_index(0)]
        pub fn set_periods(
            origin: OriginFor<T>,
            schedule_period_in_blocks: T::BlockNumber,
            voting_period_in_blocks: T::BlockNumber,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::validate_schedule_period(schedule_period_in_blocks)?;
            Self::validate_voting_period(voting_period_in_blocks, schedule_period_in_blocks)?;

            let next_block_to_process = <NextBlockToProcess<T>>::get();
            let new_slot_at_block =
                safe_add_block_numbers(next_block_to_process, schedule_period_in_blocks)
                    .map_err(|_| Error::<T>::Overflow)?;

            <SchedulePeriod<T>>::put(schedule_period_in_blocks);
            <VotingPeriod<T>>::put(voting_period_in_blocks);
            <NextSlotAtBlock<T>>::put(new_slot_at_block);

            Self::deposit_event(Event::<T>::SchedulePeriodAndVotingPeriodUpdated {
                schedule_period: schedule_period_in_blocks,
                voting_period: voting_period_in_blocks,
            });
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::record_summary_calculation(
            MAX_VALIDATOR_ACCOUNT_IDS,
            MAX_NUMBER_OF_ROOT_DATA_PER_RANGE
        ))]
        #[pallet::call_index(1)]
        pub fn record_summary_calculation(
            origin: OriginFor<T>,
            new_block_number: T::BlockNumber,
            root_hash: H256,
            ingress_counter: IngressCounter,
            validator: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            _signature: <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            ensure!(
                Self::get_ingress_counter() + 1 == ingress_counter,
                Error::<T>::InvalidIngressCounter
            );
            ensure!(AVN::<T>::is_validator(&validator.account_id), Error::<T>::InvalidKey);

            let root_range = RootRange::new(Self::get_next_block_to_process(), new_block_number);
            let root_id = RootId::new(root_range, ingress_counter);
            let expected_target_block = Self::get_target_block()?;
            let current_block_number = <system::Pallet<T>>::block_number();

            ensure!(
                Self::summary_is_neither_pending_nor_approved(&root_id.range),
                Error::<T>::SummaryPendingOrApproved
            );
            ensure!(
                !<VotesRepository<T>>::contains_key(root_id),
                Error::<T>::RootHasAlreadyBeenRegisteredForVoting
            );
            ensure!(new_block_number == expected_target_block, Error::<T>::InvalidSummaryRange);

            let quorum = AVN::<T>::quorum();
            let voting_period_end =
                safe_add_block_numbers(current_block_number, Self::voting_period())
                    .map_err(|_| Error::<T>::Overflow)?;

            <TotalIngresses<T>>::put(ingress_counter);
            <Roots<T>>::insert(
                &root_id.range,
                ingress_counter,
                RootData::new(root_hash, validator.account_id.clone(), None),
            );
            <PendingApproval<T>>::insert(root_id.range, ingress_counter);
            <VotesRepository<T>>::insert(
                root_id,
                VotingSessionData::new(
                    root_id.session_id(),
                    quorum,
                    voting_period_end,
                    current_block_number,
                ),
            );

            Self::deposit_event(Event::<T>::SummaryCalculated {
                from: root_id.range.from_block,
                to: root_id.range.to_block,
                root_hash,
                submitter: validator.account_id,
            });
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::approve_root_with_end_voting(MAX_VALIDATOR_ACCOUNT_IDS, MAX_OFFENDERS).max(
            <T as Config>::WeightInfo::approve_root_without_end_voting(MAX_VALIDATOR_ACCOUNT_IDS)
        ))]
        #[pallet::call_index(2)]
        pub fn approve_root(
            origin: OriginFor<T>,
            root_id: RootId<T::BlockNumber>,
            validator: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            approval_signature: ecdsa::Signature,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;

            let root_data = Self::try_get_root_data(&root_id)?;
            let eth_encoded_data = Self::convert_data_to_eth_compatible_encoding(&root_data)?;
            if !AVN::<T>::eth_signature_is_valid(eth_encoded_data, &validator, &approval_signature)
            {
                create_and_report_summary_offence::<T>(
                    &validator.account_id,
                    &vec![validator.account_id.clone()],
                    SummaryOffenceType::InvalidSignatureSubmitted,
                );
                return Err(avn_error::<T>::InvalidECDSASignature)?
            };

            let voting_session = Self::get_root_voting_session(&root_id);

            process_approve_vote::<T>(
                &voting_session,
                validator.account_id.clone(),
                approval_signature,
            )?;

            Self::deposit_event(Event::<T>::VoteAdded {
                voter: validator.account_id,
                root_id,
                agree_vote: true,
            });
            // TODO [TYPE: weightInfo][PRI: medium]: Return accurate weight
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::reject_root_with_end_voting(MAX_VALIDATOR_ACCOUNT_IDS, MAX_OFFENDERS).max(
            <T as Config>::WeightInfo::reject_root_without_end_voting(MAX_VALIDATOR_ACCOUNT_IDS)
        ))]
        #[pallet::call_index(3)]
        pub fn reject_root(
            origin: OriginFor<T>,
            root_id: RootId<T::BlockNumber>,
            validator: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            let voting_session = Self::get_root_voting_session(&root_id);
            process_reject_vote::<T>(&voting_session, validator.account_id.clone())?;

            Self::deposit_event(Event::<T>::VoteAdded {
                voter: validator.account_id,
                root_id,
                agree_vote: false,
            });
            // TODO [TYPE: weightInfo][PRI: medium]: Return accurate weight
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::end_voting_period_with_rejected_valid_votes(MAX_OFFENDERS).max(
            <T as Config>::WeightInfo::end_voting_period_with_approved_invalid_votes(MAX_OFFENDERS)
        ))]
        #[pallet::call_index(4)]
        pub fn end_voting_period(
            origin: OriginFor<T>,
            root_id: RootId<T::BlockNumber>,
            validator: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            //Event is deposited in end_voting because this function can get called from
            // `approve_root` or `reject_root`
            Self::end_voting(validator.account_id, &root_id)?;

            // TODO [TYPE: weightInfo][PRI: medium]: Return accurate weight
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::advance_slot_with_offence().max(
            <T as Config>::WeightInfo::advance_slot_without_offence()
        ))]
        #[pallet::call_index(5)]
        pub fn advance_slot(
            origin: OriginFor<T>,
            validator: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;

            Self::update_slot_number(validator)?;

            // TODO [TYPE: weightInfo][PRI: medium]: Return accurate weight
            Ok(())
        }

        #[pallet::weight( <T as pallet::Config>::WeightInfo::add_challenge())]
        #[pallet::call_index(6)]
        pub fn add_challenge(
            origin: OriginFor<T>,
            challenge: SummaryChallenge<T::AccountId>,
            validator: Validator<T::AuthorityId, T::AccountId>,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            ensure!(
                challenge.is_valid::<T>(
                    Self::current_slot(),
                    <frame_system::Pallet<T>>::block_number(),
                    &challenge.challengee
                ),
                Error::<T>::InvalidChallenge
            );
            // QUESTION: offence: do we slash the author of an invalid challenge?
            // I think it is probably too harsh. It may not be valid for timing reasons:
            // it arrived too early, or the slot has already moved and the validator changed

            let offender = challenge.challengee.clone();
            let challenge_type = match challenge.challenge_reason {
                SummaryChallengeReason::SlotNotAdvanced(_) =>
                    Some(SummaryOffenceType::SlotNotAdvanced),
                SummaryChallengeReason::Unknown => None,
            };

            // if this fails, it is a bug. All challenge types should have a corresponding offence
            // type except for Unknown which we should never produce
            ensure!(!challenge_type.is_none(), Error::<T>::InvalidChallenge);

            create_and_report_summary_offence::<T>(
                &validator.account_id,
                &vec![offender],
                challenge_type.expect("Already checked"),
            );

            Self::update_slot_number(validator)?;

            Self::deposit_event(Event::<T>::ChallengeAdded {
                challenge_reason: challenge.challenge_reason,
                challenger: challenge.challenger,
                challengee: challenge.challengee,
            });

            Ok(())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn offchain_worker(block_number: T::BlockNumber) {
            log::info!("🚧 🚧 Running offchain worker for block: {:?}", block_number);
            let setup_result = AVN::<T>::pre_run_setup(block_number, NAME.to_vec());
            if let Err(e) = setup_result {
                match e {
                    _ if e == DispatchError::from(avn_error::<T>::OffchainWorkerAlreadyRun) => {
                        ();
                    },
                    _ => {
                        log::error!("💔️ Unable to run offchain worker: {:?}", e);
                    },
                };

                return
            }
            let this_validator = setup_result.expect("We have a validator");

            Self::advance_slot_if_required(block_number, &this_validator);
            Self::process_summary_if_required(block_number, &this_validator);
            cast_votes_if_required::<T>(&this_validator);
            end_voting_if_required::<T>(block_number, &this_validator);
            challenge_slot_if_required::<T>(block_number, &this_validator);
        }

        // Note: this "special" function will run during every runtime upgrade. Any complicated
        // migration logic should be done in a separate function so it can be tested
        // properly.
        fn on_runtime_upgrade() -> Weight {
            let mut weight_write_counter = 0;
            sp_runtime::runtime_logger::RuntimeLogger::init();
            log::info!("ℹ️  Summary pallet data migration invoked");

            if Self::schedule_period() == 0u32.into() {
                log::info!(
                    "ℹ️  Updating SchedulePeriod to a default value of {} blocks",
                    DEFAULT_SCHEDULE_PERIOD
                );
                weight_write_counter += 1;
                <SchedulePeriod<T>>::put(<T as frame_system::Config>::BlockNumber::from(
                    DEFAULT_SCHEDULE_PERIOD,
                ));
            }

            if Self::voting_period() == 0u32.into() {
                log::info!(
                    "ℹ️  Updating VotingPeriod to a default value of {} blocks",
                    DEFAULT_VOTING_PERIOD
                );
                weight_write_counter += 1;
                <VotingPeriod<T>>::put(<T as frame_system::Config>::BlockNumber::from(
                    DEFAULT_VOTING_PERIOD,
                ));
            }

            return T::DbWeight::get().writes(weight_write_counter as u64)
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            if let Call::record_summary_calculation { .. } = call {
                return Self::record_summary_validate_unsigned(source, call)
            } else if let Call::end_voting_period { root_id, validator, signature } = call {
                let root_voting_session = Self::get_root_voting_session(root_id);
                return end_voting_period_validate_unsigned::<T>(
                    &root_voting_session,
                    validator,
                    signature,
                )
            } else if let Call::approve_root { root_id, validator, approval_signature, signature } =
                call
            {
                if !<Roots<T>>::contains_key(root_id.range, root_id.ingress_counter) {
                    return InvalidTransaction::Custom(ERROR_CODE_INVALID_ROOT_RANGE).into()
                }

                let root_voting_session = Self::get_root_voting_session(root_id);

                let root_data = Self::try_get_root_data(&root_id)
                    .map_err(|_| InvalidTransaction::Custom(ERROR_CODE_INVALID_ROOT_RANGE))?;

                let eth_encoded_data = Self::convert_data_to_eth_compatible_encoding(&root_data)
                    .map_err(|_| InvalidTransaction::Custom(ERROR_CODE_INVALID_ROOT_DATA))?;

                return approve_vote_validate_unsigned::<T>(
                    &root_voting_session,
                    validator,
                    eth_encoded_data.encode(),
                    approval_signature,
                    signature,
                )
            } else if let Call::reject_root { root_id, validator, signature } = call {
                let root_voting_session = Self::get_root_voting_session(root_id);
                return reject_vote_validate_unsigned::<T>(
                    &root_voting_session,
                    validator,
                    signature,
                )
            } else if let Call::add_challenge { challenge, validator, signature } = call {
                return add_challenge_validate_unsigned::<T>(challenge, validator, signature)
            } else if let Call::advance_slot { .. } = call {
                return Self::advance_slot_validate_unsigned(source, call)
            } else {
                return InvalidTransaction::Call.into()
            }
        }
    }
    impl<T: Config> Pallet<T> {
        fn validate_schedule_period(schedule_period_in_blocks: T::BlockNumber) -> DispatchResult {
            ensure!(
                schedule_period_in_blocks >= MIN_SCHEDULE_PERIOD.into(),
                Error::<T>::SchedulePeriodIsTooShort
            );

            Ok(())
        }

        fn validate_voting_period(
            voting_period_in_blocks: T::BlockNumber,
            schedule_period_in_blocks: T::BlockNumber,
        ) -> DispatchResult {
            ensure!(
                voting_period_in_blocks >= MIN_VOTING_PERIOD.into(),
                Error::<T>::VotingPeriodIsTooShort
            );
            ensure!(
                voting_period_in_blocks < schedule_period_in_blocks,
                Error::<T>::VotingPeriodIsEqualOrLongerThanSchedulePeriod
            );
            ensure!(
                voting_period_in_blocks <= MAX_VOTING_PERIOD.into(),
                Error::<T>::VotingPeriodIsTooLong
            );
            Ok(())
        }

        pub fn grace_period_elapsed(block_number: T::BlockNumber) -> bool {
            let diff = safe_sub_block_numbers::<T::BlockNumber>(
                block_number,
                Self::block_number_for_next_slot(),
            )
            .unwrap_or(0u32.into());
            return diff > T::AdvanceSlotGracePeriod::get()
        }

        // Check if this validator is allowed
        // the slot's validator is challenged if it does not advance the slot inside the challenge
        // window. But this challenge will be checked later than when it was submitted, so it is
        // possible storage has changed by then. To prevent the validator escape the challenge, we
        // can allow it this change only inside the challenge window. Other validators can however
        // move the slot after the challenge window.
        pub fn validator_can_advance_slot(
            validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
        ) -> DispatchResult {
            let current_block_number = <frame_system::Pallet<T>>::block_number();
            ensure!(
                current_block_number >= Self::block_number_for_next_slot(),
                Error::<T>::TooEarlyToAdvance
            );

            let current_slot_validator =
                Self::slot_validator().ok_or(Error::<T>::CurrentSlotValidatorNotFound)?;

            if Self::grace_period_elapsed(current_block_number) {
                if validator.account_id == current_slot_validator {
                    return Err(Error::<T>::GracePeriodElapsed)?
                }
            } else {
                if validator.account_id != current_slot_validator {
                    return Err(Error::<T>::WrongValidator)?
                }
            }

            Ok(())
        }

        pub fn update_slot_number(
            validator: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
        ) -> DispatchResult {
            Self::validator_can_advance_slot(&validator)?;
            // QUESTION: should we slash a validator who tries to advance the slot when it is not
            // their turn? This code is always called inside an unsigned transaction, so
            // in consensus. We can raise offences here.
            Self::register_offence_if_no_summary_created_in_slot(&validator);

            let new_slot_number =
                safe_add_block_numbers::<T::BlockNumber>(Self::current_slot(), 1u32.into())
                    .map_err(|_| Error::<T>::Overflow)?;

            let new_validator_account_id = AVN::<T>::calculate_primary_validator(new_slot_number)?;

            let next_slot_start_block = safe_add_block_numbers::<T::BlockNumber>(
                Self::block_number_for_next_slot(),
                Self::schedule_period(),
            )
            .map_err(|_| Error::<T>::Overflow)?;

            <CurrentSlot<T>>::put(new_slot_number);
            <CurrentSlotsValidator<T>>::put(new_validator_account_id.clone());
            <NextSlotAtBlock<T>>::put(next_slot_start_block);

            Self::deposit_event(Event::<T>::SlotAdvanced {
                advanced_by: validator.account_id,
                new_slot: new_slot_number,
                slot_validator: new_validator_account_id,
                slot_end: next_slot_start_block,
            });

            Ok(())
        }

        pub fn get_root_voting_session(
            root_id: &RootId<T::BlockNumber>,
        ) -> Box<dyn VotingSessionManager<T::AccountId, T::BlockNumber>> {
            return Box::new(RootVotingSession::<T>::new(root_id))
                as Box<dyn VotingSessionManager<T::AccountId, T::BlockNumber>>
        }

        // This can be called by other validators to verify the root hash
        pub fn compute_root_hash(
            from_block: T::BlockNumber,
            to_block: T::BlockNumber,
        ) -> Result<H256, DispatchError> {
            let from_block_number: u32 = TryInto::<u32>::try_into(from_block)
                .map_err(|_| Error::<T>::ErrorConvertingBlockNumber)?;
            let to_block_number: u32 = TryInto::<u32>::try_into(to_block)
                .map_err(|_| Error::<T>::ErrorConvertingBlockNumber)?;

            let mut url_path = "roothash/".to_string();
            url_path.push_str(&from_block_number.to_string());
            url_path.push_str(&"/".to_string());
            url_path.push_str(&to_block_number.to_string());

            let response = AVN::<T>::get_data_from_service(url_path);

            if let Err(e) = response {
                log::error!("💔️ Error getting summary data from external service: {:?}", e);
                return Err(Error::<T>::ErrorGettingSummaryDataFromService)?
            }

            let root_hash = Self::validate_response(response.expect("checked for error"))?;
            log::trace!(target: "avn", "🥽 Calculated root hash {:?} for range [{:?}, {:?}]", &root_hash, &from_block_number, &to_block_number);

            return Ok(root_hash)
        }

        pub fn create_root_lock_name(block_number: T::BlockNumber) -> Vec<u8> {
            let mut name = b"create_summary::".to_vec();
            name.extend_from_slice(&mut block_number.encode());
            name
        }

        pub fn get_advance_slot_lock_name(block_number: T::BlockNumber) -> Vec<u8> {
            let mut name = b"advance_slot::".to_vec();
            name.extend_from_slice(&mut block_number.encode());
            name
        }

        pub fn convert_data_to_eth_compatible_encoding(
            root_data: &RootData<T::AccountId>,
        ) -> Result<String, DispatchError> {
            let root_hash = *root_data.root_hash.as_fixed_bytes();
            let tx_id = match root_data.tx_id {
                None => EMPTY_ROOT_TRANSACTION_ID,
                _ => *root_data
                    .tx_id
                    .as_ref()
                    .expect("Non-Empty roots have a reserved TransactionId"),
            };
            let expiry = T::BridgePublisher::get_eth_tx_lifetime_secs();
            let encoded_data = util::encode_summary_data(&root_hash, expiry, tx_id);
            let msg_hash = keccak_256(&encoded_data);

            Ok(hex::encode(msg_hash))
        }

        pub fn sign_root_for_ethereum(
            root_id: &RootId<T::BlockNumber>,
        ) -> Result<(String, ecdsa::Signature), DispatchError> {
            log::info!("HELP SIGN ROOT FOR ETHEREUM !!!");
            let root_data = Self::try_get_root_data(&root_id)?;
            log::info!("HELP ROOT DATA !!! {:?}", root_data);
            let data = Self::convert_data_to_eth_compatible_encoding(&root_data)?;

            return Ok((
                data.clone(),
                AVN::<T>::request_ecdsa_signature_from_external_service(&data)?,
            ))
        }

        pub fn advance_slot_if_required(
            block_number: T::BlockNumber,
            this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
        ) {
            let current_slot_validator = Self::slot_validator();
            if current_slot_validator.is_none() {
                log::error!(
                    "💔 Current slot validator is not found. Cannot advance slot for block: {:?}",
                    block_number
                );
                return
            }

            if this_validator.account_id == current_slot_validator.expect("Checked for none") &&
                block_number >= Self::block_number_for_next_slot()
            {
                let advance_slot_lock_name = Self::get_advance_slot_lock_name(Self::current_slot());
                let mut lock = AVN::<T>::get_ocw_locker(&advance_slot_lock_name);

                // Protect against sending more than once. When guard is out of scope the lock will
                // be released.
                if let Ok(guard) = lock.try_lock() {
                    let result = Self::dispatch_advance_slot(this_validator);
                    if let Err(e) = result {
                        log::warn!("💔️ Error starting a new summary creation slot: {:?}", e);
                        //free the lock so we can potentially retry
                        drop(guard);
                        return
                    }

                    // If there are no errors, keep the lock to prevent doing the same logic again
                    guard.forget();
                };
            }
        }
        // called from OCW - no storage changes allowed here
        pub fn process_summary_if_required(
            block_number: T::BlockNumber,
            this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
        ) {
            let target_block = Self::get_target_block();
            if target_block.is_err() {
                log::error!("💔️ Error getting target block.");
                return
            }
            let last_block_in_range = target_block.expect("Valid block number");

            if Self::can_process_summary(block_number, last_block_in_range, this_validator) {
                let root_lock_name = Self::create_root_lock_name(last_block_in_range);
                let mut lock = AVN::<T>::get_ocw_locker(&root_lock_name);

                // Protect against sending more than once. When guard is out of scope the lock will
                // be released.
                if let Ok(guard) = lock.try_lock() {
                    log::warn!(
                        "ℹ️  Processing summary for range {:?} - {:?}. Slot {:?}",
                        Self::get_next_block_to_process(),
                        last_block_in_range,
                        Self::current_slot()
                    );

                    let summary = Self::process_summary(last_block_in_range, this_validator);

                    if let Err(e) = summary {
                        log::warn!("💔️ Error processing summary: {:?}", e);
                        //free the lock so we can potentially retry
                        drop(guard);
                        return
                    }

                    // If there are no errors, keep the lock to prevent doing the same logic again
                    guard.forget();
                };
            }
        }

        fn register_offence_if_no_summary_created_in_slot(
            reporter: &Validator<T::AuthorityId, T::AccountId>,
        ) {
            if Self::last_summary_slot() < Self::current_slot() {
                let maybe_current_slot_validator = Self::slot_validator();
                if maybe_current_slot_validator.is_none() {
                    log::error!(
                        "💔 Current slot validator is not found. Unable to register offence"
                    );
                    return
                }
                let current_slot_validator =
                    maybe_current_slot_validator.expect("Checked for none");

                create_and_report_summary_offence::<T>(
                    &reporter.account_id,
                    &vec![current_slot_validator.clone()],
                    SummaryOffenceType::NoSummaryCreated,
                );

                Self::deposit_event(Event::<T>::SummaryNotPublishedOffence {
                    challengee: current_slot_validator,
                    void_slot: Self::current_slot(),
                    last_published: Self::last_summary_slot(),
                    end_vote: Self::block_number_for_next_slot(),
                });
            }
        }

        // called from OCW - no storage changes allowed here
        fn can_process_summary(
            current_block_number: T::BlockNumber,
            last_block_in_range: T::BlockNumber,
            this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
        ) -> bool {
            if OcwLock::is_locked::<frame_system::Pallet<T>>(&Self::create_root_lock_name(
                last_block_in_range,
            )) {
                return false
            }

            let target_block_with_buffer =
                safe_add_block_numbers(last_block_in_range, T::MinBlockAge::get());

            if target_block_with_buffer.is_err() {
                log::warn!(
                    "💔️ Error checking if we can process a summary for blocks {:?} to {:?}",
                    current_block_number,
                    last_block_in_range
                );

                return false
            }
            let target_block_with_buffer = target_block_with_buffer.expect("Already checked");

            let root_range = RootRange::new(Self::get_next_block_to_process(), last_block_in_range);

            let current_slot_validator = Self::slot_validator();
            let is_slot_validator = current_slot_validator.is_some() &&
                this_validator.account_id == current_slot_validator.expect("checked for none");
            let slot_is_active = current_block_number < Self::block_number_for_next_slot();
            let blocks_are_old_enough = current_block_number > target_block_with_buffer;

            return is_slot_validator &&
                slot_is_active &&
                blocks_are_old_enough &&
                Self::summary_is_neither_pending_nor_approved(&root_range)
        }

        // called from OCW - no storage changes allowed here
        pub fn process_summary(
            last_block_in_range: T::BlockNumber,
            validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
        ) -> DispatchResult {
            let root_hash =
                Self::compute_root_hash(Self::get_next_block_to_process(), last_block_in_range)?;
            Self::record_summary(last_block_in_range, root_hash, validator)?;

            Ok(())
        }

        // called from OCW - no storage changes allowed here
        fn record_summary(
            last_processed_block_number: T::BlockNumber,
            root_hash: H256,
            validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
        ) -> DispatchResult {
            let ingress_counter = Self::get_ingress_counter() + 1; // default value in storage is 0, so first root_hash has counter 1

            let signature = validator
                .key
                .sign(
                    &(
                        UPDATE_BLOCK_NUMBER_CONTEXT,
                        root_hash,
                        ingress_counter,
                        last_processed_block_number,
                    )
                        .encode(),
                )
                .ok_or(Error::<T>::ErrorSigning)?;

            log::trace!(
                target: "avn",
                "🖊️  Worker records summary calculation: {:?} last processed block {:?} ingress: {:?}]",
                &root_hash,
                &last_processed_block_number,
                &ingress_counter
            );

            SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                Call::record_summary_calculation {
                    new_block_number: last_processed_block_number,
                    root_hash,
                    ingress_counter,
                    validator: validator.clone(),
                    signature,
                }
                .into(),
            )
            .map_err(|_| Error::<T>::ErrorSubmittingTransaction)?;

            Ok(())
        }

        fn dispatch_advance_slot(
            validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
        ) -> DispatchResult {
            let signature = validator
                .key
                .sign(&(ADVANCE_SLOT_CONTEXT, Self::current_slot()).encode())
                .ok_or(Error::<T>::ErrorSigning)?;

            SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                Call::advance_slot { validator: validator.clone(), signature }.into(),
            )
            .map_err(|_| Error::<T>::ErrorSubmittingTransaction)?;

            Ok(())
        }

        pub fn get_target_block() -> Result<T::BlockNumber, Error<T>> {
            let end_block_number = safe_add_block_numbers::<T::BlockNumber>(
                Self::get_next_block_to_process(),
                Self::schedule_period(),
            )
            .map_err(|_| Error::<T>::Overflow)?;

            if Self::get_next_block_to_process() == 0u32.into() {
                return Ok(end_block_number)
            }

            Ok(safe_sub_block_numbers::<T::BlockNumber>(end_block_number, 1u32.into())
                .map_err(|_| Error::<T>::Overflow)?)
        }

        fn validate_response(response: Vec<u8>) -> Result<H256, Error<T>> {
            if response.len() != 64 {
                log::error!("❌ Root hash is not valid: {:?}", response);
                return Err(Error::<T>::InvalidRootHashLength)?
            }

            let root_hash = core::str::from_utf8(&response);
            if let Err(e) = root_hash {
                log::error!("❌ Error converting root hash bytes to string: {:?}", e);
                return Err(Error::<T>::InvalidUTF8Bytes)?
            }

            let mut data: [u8; 32] = [0; 32];
            hex::decode_to_slice(root_hash.expect("Checked for error"), &mut data[..])
                .map_err(|_| Error::<T>::InvalidHexString)?;

            return Ok(H256::from_slice(&data))
        }

        pub fn end_voting(
            reporter: T::AccountId,
            root_id: &RootId<T::BlockNumber>,
        ) -> DispatchResult {
            let voting_session = Self::get_root_voting_session(&root_id);

            ensure!(voting_session.is_valid(), Error::<T>::VotingSessionIsNotValid);

            let vote = Self::get_vote(root_id);
            ensure!(Self::can_end_vote(&vote), Error::<T>::ErrorEndingVotingPeriod);

            let root_is_approved = vote.is_approved();

            let root_data = Self::try_get_root_data(&root_id)?;
            if root_is_approved {
                if root_data.root_hash != Self::empty_root() {
                    let function_name: &[u8] = b"publishRoot";
                    let params =
                        vec![(b"bytes32".to_vec(), root_data.root_hash.as_fixed_bytes().to_vec())];
                    let tx_id = T::BridgePublisher::publish(function_name, &params)
                        .map_err(|e| DispatchError::Other(e.into()))?;

                    <Roots<T>>::mutate(root_id.range, root_id.ingress_counter, |root| {
                        root.tx_id = Some(tx_id)
                    });

                    // There are a couple possible reasons for failure.
                    // 1. We fail before sending to T1: likely a bug on our part
                    // 2. Quorum mismatch. There is no guarantee that between accepting a root and
                    // submitting it to T1, the tier2 session hasn't changed and with it
                    // the quorum, making ethereum-transactions reject it
                    // In either case, we should not slash anyone.
                }
                // If we get here, then we did not get an error when submitting to T1.

                create_and_report_summary_offence::<T>(
                    &reporter,
                    &vote.nays,
                    SummaryOffenceType::RejectedValidRoot,
                );

                let next_block_to_process =
                    safe_add_block_numbers::<T::BlockNumber>(root_id.range.to_block, 1u32.into())
                        .map_err(|_| Error::<T>::Overflow)?;

                <NextBlockToProcess<T>>::put(next_block_to_process);
                <Roots<T>>::mutate(root_id.range, root_id.ingress_counter, |root| {
                    root.is_validated = true
                });
                <SlotOfLastPublishedSummary<T>>::put(Self::current_slot());

                Self::deposit_event(Event::<T>::SummaryRootValidated {
                    root_hash: root_data.root_hash,
                    ingress_counter: root_id.ingress_counter,
                    block_range: root_id.range,
                });
            } else {
                // We didn't get enough votes to approve this root

                let root_creator =
                    root_data.added_by.ok_or(Error::<T>::CurrentSlotValidatorNotFound)?;
                create_and_report_summary_offence::<T>(
                    &reporter,
                    &vec![root_creator],
                    SummaryOffenceType::CreatedInvalidRoot,
                );

                create_and_report_summary_offence::<T>(
                    &reporter,
                    &vote.ayes,
                    SummaryOffenceType::ApprovedInvalidRoot,
                );
            }

            <PendingApproval<T>>::remove(root_id.range);

            // When we get here, the root's voting session has ended and it has been removed from
            // PendingApproval If the root was approved, it is now marked as validated.
            // Otherwise, it stays false. If there was an error when submitting to T1, none of
            // this happened and it is still pending and not validated. In either case, the whole
            // voting history remains in storage

            // NOTE: when SYS-152 work is added here, root_range could exist several times in the
            // voting history, since a root_range that is rejected must eventually be
            // submitted again. But at any given time, there should be a single instance
            // of root_range in the PendingApproval queue. It is possible to keep
            // several instances of root_range in the Roots repository. But that should
            // not change the logic in this area: we should still validate an approved
            // (root_range, counter) and remove this pair from PendingApproval if no
            // errors occur.

            Self::deposit_event(Event::<T>::VotingEnded {
                root_id: *root_id,
                vote_approved: root_is_approved,
            });

            Ok(())
        }

        fn can_end_vote(vote: &VotingSessionData<T::AccountId, T::BlockNumber>) -> bool {
            return vote.has_outcome() ||
                <system::Pallet<T>>::block_number() >= vote.end_of_voting_period
        }

        fn record_summary_validate_unsigned(
            _source: TransactionSource,
            call: &Call<T>,
        ) -> TransactionValidity {
            if let Call::record_summary_calculation {
                new_block_number,
                root_hash,
                ingress_counter,
                validator,
                signature,
            } = call
            {
                let current_slot_validator = Self::slot_validator();
                if current_slot_validator.is_none() ||
                    validator.account_id != current_slot_validator.expect("checked for none")
                {
                    return InvalidTransaction::Custom(ERROR_CODE_VALIDATOR_IS_NOT_PRIMARY).into()
                }

                let signed_data =
                    &(UPDATE_BLOCK_NUMBER_CONTEXT, root_hash, ingress_counter, new_block_number);
                if !AVN::<T>::signature_is_valid(signed_data, &validator, signature) {
                    return InvalidTransaction::BadProof.into()
                };

                return ValidTransaction::with_tag_prefix("Summary")
                    .priority(TransactionPriority::max_value())
                    .and_provides(vec![
                        (UPDATE_BLOCK_NUMBER_CONTEXT, root_hash, ingress_counter).encode()
                    ])
                    .longevity(64_u64)
                    .propagate(true)
                    .build()
            }

            return InvalidTransaction::Call.into()
        }

        fn advance_slot_validate_unsigned(
            _source: TransactionSource,
            call: &Call<T>,
        ) -> TransactionValidity {
            if let Call::advance_slot { validator, signature } = call {
                let current_slot_validator = Self::slot_validator();
                if current_slot_validator.is_none() ||
                    validator.account_id != current_slot_validator.expect("checked for none")
                {
                    return InvalidTransaction::Custom(ERROR_CODE_VALIDATOR_IS_NOT_PRIMARY).into()
                }

                // QUESTION: slash here? If we check the signature validity first, then fail the
                // check for slot_validator we would prove someone tried to advance
                // the slot outside their turn. Should this be slashable?

                let current_slot = Self::current_slot();
                let signed_data = &(ADVANCE_SLOT_CONTEXT, current_slot);
                if !AVN::<T>::signature_is_valid(signed_data, &validator, signature) {
                    return InvalidTransaction::BadProof.into()
                };

                return ValidTransaction::with_tag_prefix("Summary")
                    .priority(TransactionPriority::max_value())
                    .and_provides(vec![(ADVANCE_SLOT_CONTEXT, current_slot).encode()])
                    .longevity(64_u64)
                    .propagate(true)
                    .build()
            }

            return InvalidTransaction::Call.into()
        }

        fn empty_root() -> H256 {
            return H256::from_slice(&[0; 32])
        }

        fn summary_is_neither_pending_nor_approved(root_range: &RootRange<T::BlockNumber>) -> bool {
            let has_been_approved =
                <Roots<T>>::iter_prefix_values(root_range).any(|root| root.is_validated);
            let is_pending = <PendingApproval<T>>::contains_key(root_range);

            return !is_pending && !has_been_approved
        }

        pub fn try_get_root_data(
            root_id: &RootId<T::BlockNumber>,
        ) -> Result<RootData<T::AccountId>, Error<T>> {
            if <Roots<T>>::contains_key(root_id.range, root_id.ingress_counter) {
                return Ok(<Roots<T>>::get(root_id.range, root_id.ingress_counter))
            }

            Err(Error::<T>::RootDataNotFound)?
        }
    }
}

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Debug, Eq, TypeInfo, MaxEncodedLen)]
pub struct RootId<BlockNumber: AtLeast32Bit> {
    pub range: RootRange<BlockNumber>,
    pub ingress_counter: IngressCounter,
}

impl<BlockNumber: AtLeast32Bit + Encode> RootId<BlockNumber> {
    fn new(range: RootRange<BlockNumber>, ingress_counter: IngressCounter) -> Self {
        return RootId::<BlockNumber> { range, ingress_counter }
    }

    fn session_id(&self) -> BoundedVec<u8, VotingSessionIdBound> {
        BoundedVec::truncate_from(self.encode())
    }
}

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Debug, Eq, TypeInfo, MaxEncodedLen)]
pub struct RootRange<BlockNumber: AtLeast32Bit> {
    pub from_block: BlockNumber,
    pub to_block: BlockNumber,
}

impl<BlockNumber: AtLeast32Bit> RootRange<BlockNumber> {
    fn new(from_block: BlockNumber, to_block: BlockNumber) -> Self {
        return RootRange::<BlockNumber> { from_block, to_block }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq, TypeInfo, MaxEncodedLen)]
pub struct RootData<AccountId> {
    pub root_hash: H256,
    pub added_by: Option<AccountId>,
    pub is_validated: bool, // This is set to true when 2/3 of validators approve it
    pub is_finalised: bool, /* This is set to true when EthEvents confirms Tier1 has received
                             * the root */
    pub tx_id: Option<EthereumTransactionId>, /* This is the TransacionId that will be used to
                                               * submit
                                               * the tx */
}

impl<AccountId> RootData<AccountId> {
    fn new(
        root_hash: H256,
        added_by: AccountId,
        transaction_id: Option<EthereumTransactionId>,
    ) -> Self {
        return RootData::<AccountId> {
            root_hash,
            added_by: Some(added_by),
            is_validated: false,
            is_finalised: false,
            tx_id: transaction_id,
        }
    }
}

impl<AccountId> Default for RootData<AccountId> {
    fn default() -> Self {
        Self {
            root_hash: H256::zero(),
            added_by: None,
            is_validated: false,
            is_finalised: false,
            tx_id: None,
        }
    }
}
impl<T: Config> OnBridgePublisherResult for Pallet<T> {
    fn process_result(tx_id: u32, succeeded: bool) -> DispatchResult {
        if succeeded {
            log::info!("✅  Transaction with ID {} was successfully published to Ethereum.", tx_id);
        } else {
            log::error!("❌ Transaction with ID {} failed to publish to Ethereum.", tx_id);
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/tests_vote.rs"]
mod tests_vote;

#[cfg(test)]
#[path = "tests/tests_validate_unsigned.rs"]
mod tests_validate_unsigned;

#[cfg(test)]
#[path = "tests/tests_slot_logic.rs"]
mod tests_slots;

#[cfg(test)]
#[path = "tests/tests_challenge.rs"]
mod tests_challenge;

#[cfg(test)]
#[path = "tests/tests_set_periods.rs"]
mod tests_set_periods;

#[cfg(test)]
#[path = "tests/test_ocw_locks.rs"]
mod test_ocw_locks;

// TODO: Add unit tests for setting schedule period and voting period
