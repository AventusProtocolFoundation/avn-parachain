//! # Aventus Finality Tracker Pallet
//!
//! This pallet is responsible for tracking the latest finalised block and storing it on chain
//!
//! All validators are expected to periodically send their opinion of what is the latest finalised
//! block, and this pallet will select the highest finalised block seen by 2/3 or more of the
//! validators.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use codec::{Decode, Encode, MaxEncodedLen};
use sp_runtime::{
    offchain::storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
    scale_info::TypeInfo,
    traits::{AtLeast32Bit, Member, Zero},
    transaction_validity::{
        InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
        ValidTransaction,
    },
};
use sp_std::{cmp, prelude::*};

use frame_support::{log, traits::Get};
use frame_system::offchain::{SendTransactionTypes, SubmitTransaction};
pub use pallet::*;
use pallet_avn::{self as avn, Error as avn_error, FinalisedBlockChecker};
use sp_application_crypto::RuntimeAppPublic;
use sp_avn_common::{
    event_types::Validator,
    offchain_worker_storage_lock::{self as OcwLock},
};

const NAME: &'static [u8; 12] = b"avn-finality";
const UPDATE_FINALISED_BLOCK_NUMBER_CONTEXT: &'static [u8] =
    b"update_finalised_block_number_signing_context";

const FINALISED_BLOCK_END_POINT: &str = "latest_finalised_block";

// used in benchmarks and weights calculation only
const MAX_VALIDATOR_ACCOUNT_IDS: u32 = 10;

pub type AVN<T> = avn::Pallet<T>;

#[cfg(test)]
mod mock;

mod benchmarking;

pub mod default_weights;
pub use default_weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config:
        SendTransactionTypes<Call<Self>> + frame_system::Config + avn::Config
    {
        /// Overarching event type
        type Event: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::Event>
            + Into<<Self as frame_system::Config>::Event>;
        /// The number of block we can keep the calculated finalised block, before recalculating it
        /// again.
        type CacheAge: Get<Self::BlockNumber>;
        /// The interval, in block number, of sumbitting updates
        type SubmissionInterval: Get<Self::BlockNumber>;
        /// The delay after which point things become suspicious. Default is 100.
        type ReportLatency: Get<Self::BlockNumber>;
        /// Weight information for the extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    // #[pallet::without_storage_info]
    // TODO review the above - look at replacing all unbounded vectors so we can use this feature
    pub struct Pallet<T>(_);

    #[pallet::event]
    /// This attribute generate the function `deposit_event` to deposit one of this pallet event,
    /// it is optional, it is also possible to provide a custom implementation.
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// BlockNumber is the new finalised block number
        FinalisedBlockUpdated { block: T::BlockNumber },
        /// BlockNumber is the last block number data was updated
        FinalisedBlockUpdateStalled { block: T::BlockNumber },
    }

    #[pallet::storage]
    #[pallet::getter(fn latest_finalised_block_number)]
    pub type LatestFinalisedBlock<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_finalised_block_update)]
    pub type LastFinalisedBlockUpdate<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_finalised_block_submission)]
    pub type LastFinalisedBlockSubmission<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn submissions)]
    pub type SubmittedBlockNumbers<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, SubmissionData<T::BlockNumber>, ValueQuery>;

    #[pallet::error]
    pub enum Error<T> {
        /// Finalized height above block number
        InvalidSubmission,
        ErrorGettingDataFromService,
        InvalidResponseType,
        ErrorDecodingResponse,
        ErrorSigning,
        ErrorSubmittingTransaction,
        SubmitterNotAValidator,
        NotAllowedToSubmitAtThisTime,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(
            <T as pallet::Config>::WeightInfo::submit_latest_finalised_block_number(MAX_VALIDATOR_ACCOUNT_IDS)
        )]
        pub fn submit_latest_finalised_block_number(
            origin: OriginFor<T>,
            new_finalised_block_number: T::BlockNumber,
            validator: Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
            _signature: <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            ensure!(
                AVN::<T>::is_validator(&validator.account_id),
                Error::<T>::SubmitterNotAValidator
            );
            ensure!(
                new_finalised_block_number > Self::latest_finalised_block_number(),
                Error::<T>::InvalidSubmission
            );
            ensure!(
                Self::is_submission_valid(&validator),
                Error::<T>::NotAllowedToSubmitAtThisTime
            );

            let current_block_number = <frame_system::Pallet<T>>::block_number();
            let submission_data =
                SubmissionData::new(new_finalised_block_number, current_block_number);

            // No errors allowed below this line
            Self::record_submission(&validator.account_id, submission_data);
            LastFinalisedBlockSubmission::<T>::put(current_block_number);

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

            Self::submit_finalised_block_if_required(&this_validator);
        }

        fn on_finalize(_n: T::BlockNumber) {
            Self::update_latest_finalised_block_if_required();
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            if let Call::submit_latest_finalised_block_number {
                new_finalised_block_number,
                validator,
                signature,
            } = call
            {
                let signed_data =
                    &(UPDATE_FINALISED_BLOCK_NUMBER_CONTEXT, new_finalised_block_number);
                if !AVN::<T>::signature_is_valid(signed_data, &validator, signature) {
                    return InvalidTransaction::BadProof.into()
                };

                ValidTransaction::with_tag_prefix("AvnFinalityTracker")
                    .priority(TransactionPriority::max_value())
                    .and_provides(vec![(new_finalised_block_number, validator).encode()])
                    .longevity(10) // after 10 block we have to revalidate this transaction
                    .propagate(true)
                    .build()
            } else {
                return InvalidTransaction::Call.into()
            }
        }
    }
}

impl<T: Config> Pallet<T> {
    /// This function will only update the finalised block if there are 2/3rd or more submissions
    /// from distinct validators
    pub fn update_latest_finalised_block_if_required() {
        let quorum = AVN::<T>::calculate_two_third_quorum();
        let current_block_number = <frame_system::Pallet<T>>::block_number();
        let last_finalised_block_submission = Self::last_finalised_block_submission();

        let quorum_is_reached = SubmittedBlockNumbers::<T>::iter().count() as u32 >= quorum;
        let block_is_stale =
            current_block_number > Self::last_finalised_block_update() + T::CacheAge::get();
        let new_submissions_available =
            last_finalised_block_submission > Self::last_finalised_block_update();

        let can_update = quorum_is_reached && block_is_stale && new_submissions_available;

        if can_update {
            let calculated_finalised_block = Self::calculate_finalised_block(quorum);

            if calculated_finalised_block > Self::latest_finalised_block_number() {
                LastFinalisedBlockUpdate::<T>::put(current_block_number);
                LatestFinalisedBlock::<T>::put(calculated_finalised_block);
                Self::deposit_event(Event::<T>::FinalisedBlockUpdated {
                    block: calculated_finalised_block,
                });
            }

            // check if there is something wrong with submissions in general and notify via an event
            if current_block_number - last_finalised_block_submission > T::ReportLatency::get() {
                Self::deposit_event(Event::<T>::FinalisedBlockUpdateStalled {
                    block: last_finalised_block_submission,
                });
            }
        }
    }

    /// This method assumes all validation (such as quorum) has passed before being called.
    fn calculate_finalised_block(quorum: u32) -> T::BlockNumber {
        let mut block_candidates = vec![];
        let mut removed_validators = vec![];

        for (validator_account_id, submission) in <SubmittedBlockNumbers<T>>::iter() {
            let validator_is_active = AVN::<T>::is_validator(&validator_account_id);

            if submission.finalised_block > <T as frame_system::Config>::BlockNumber::zero() &&
                validator_is_active
            {
                block_candidates.push(submission.finalised_block);
            }

            // Keep track and remove any inactive validators
            if !validator_is_active {
                removed_validators.push(validator_account_id);
            }
        }

        removed_validators
            .iter()
            .for_each(|val| SubmittedBlockNumbers::<T>::remove(&val));

        block_candidates.sort();
        let can_be_ignored = block_candidates.len().saturating_sub(quorum as usize);
        return block_candidates[can_be_ignored]
    }

    fn record_submission(
        submitter: &T::AccountId,
        submission_data: SubmissionData<T::BlockNumber>,
    ) {
        if SubmittedBlockNumbers::<T>::contains_key(submitter) {
            SubmittedBlockNumbers::<T>::mutate(submitter, |data| *data = submission_data);
        } else {
            SubmittedBlockNumbers::<T>::insert(submitter, submission_data);
        }
    }

    fn is_submission_valid(
        submitter: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
    ) -> bool {
        let has_submitted_before = SubmittedBlockNumbers::<T>::contains_key(&submitter.account_id);

        if has_submitted_before {
            let last_submission = Self::submissions(&submitter.account_id).submitted_at_block;
            return <frame_system::Pallet<T>>::block_number() >
                last_submission + T::SubmissionInterval::get()
        }

        return true
    }

    // Called from OCW, no storage changes allowed
    fn submit_finalised_block_if_required(
        this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
    ) {
        if Self::can_submit_finalised_block(this_validator) == false {
            return
        }

        let finalised_block_result = Self::get_finalised_block_from_external_service();
        if let Err(ref e) = finalised_block_result {
            log::error!("💔 Error getting finalised block from external service: {:?}", e);
            return
        }
        let calculated_finalised_block = finalised_block_result.expect("checked for errors");

        if calculated_finalised_block <= Self::latest_finalised_block_number() {
            // Only submit if the calculated value is greater than the current value
            return
        }

        // send a transaction on chain with the latest finalised block data. We shouldn't have any
        // sig re-use issue here because new block number must be > current finalised block
        // number
        let signature = this_validator
            .key
            .sign(&(UPDATE_FINALISED_BLOCK_NUMBER_CONTEXT, calculated_finalised_block).encode())
            .ok_or(Error::<T>::ErrorSigning);

        if let Err(ref e) = signature {
            log::error!("💔 Error signing `submit finalised block` tranaction: {:?}", e);
            return
        }

        let result = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
            Call::submit_latest_finalised_block_number {
                new_finalised_block_number: calculated_finalised_block,
                validator: this_validator.clone(),
                signature: signature.expect("checked for errors"),
            }
            .into(),
        )
        .map_err(|_| Error::<T>::ErrorSubmittingTransaction);

        if let Err(e) = result {
            log::error!("💔 Error sending transaction to submit finalised block: {:?}", e);
            return
        }

        Self::set_last_finalised_block_submission_in_local_storage(calculated_finalised_block);
    }

    // Called from OCW, no storage changes allowed
    fn can_submit_finalised_block(
        this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
    ) -> bool {
        let has_submitted_before =
            SubmittedBlockNumbers::<T>::contains_key(&this_validator.account_id);

        if has_submitted_before {
            let last_submission_in_state =
                Self::submissions(&this_validator.account_id).submitted_at_block;
            let last_submission_in_local_storage =
                Self::get_last_finalised_block_submission_from_local_storage();
            let last_submission =
                cmp::max(last_submission_in_state, last_submission_in_local_storage);

            return <frame_system::Pallet<T>>::block_number() >
                last_submission + T::SubmissionInterval::get()
        }

        return true
    }

    // Called from OCW, no storage changes allowed
    fn get_finalised_block_from_external_service() -> Result<T::BlockNumber, Error<T>> {
        let response = AVN::<T>::get_data_from_service(FINALISED_BLOCK_END_POINT.to_string())
            .map_err(|_| Error::<T>::ErrorGettingDataFromService)?;

        let finalised_block_bytes =
            hex::decode(&response).map_err(|_| Error::<T>::InvalidResponseType)?;
        let finalised_block = u32::decode(&mut &finalised_block_bytes[..])
            .map_err(|_| Error::<T>::ErrorDecodingResponse)?;
        let latest_finalised_block_number = T::BlockNumber::from(finalised_block);

        return Ok(latest_finalised_block_number)
    }

    fn get_persistent_local_storage_name() -> OcwLock::PersistentId {
        return b"last_finalised_block_submission::".to_vec()
    }

    // TODO: Try to move to offchain_worker_storage_locks
    // Called from an OCW, no state changes allowed
    fn get_last_finalised_block_submission_from_local_storage() -> T::BlockNumber {
        let local_storage_key = Self::get_persistent_local_storage_name();
        let stored_value = StorageValueRef::persistent(&local_storage_key).get();
        let last_finalised_block_submission = match stored_value {
            // If the value is found
            Ok(Some(block)) => block,
            // In every other case return 0.
            _ => <T as frame_system::Config>::BlockNumber::zero(),
        };

        return last_finalised_block_submission
    }

    // TODO: Try to move to offchain_worker_storage_locks
    // Called from an OCW, no state changes allowed
    fn set_last_finalised_block_submission_in_local_storage(last_submission: T::BlockNumber) {
        const INVALID_VALUE: () = ();

        let local_storage_key = Self::get_persistent_local_storage_name();
        let val = StorageValueRef::persistent(&local_storage_key);
        let result =
            val.mutate(|last_run: Result<Option<T::BlockNumber>, StorageRetrievalError>| {
                match last_run {
                    Ok(Some(block)) if block >= last_submission => Err(INVALID_VALUE),
                    _ => Ok(last_submission),
                }
            });

        match result {
            Err(MutateStorageError::ValueFunctionFailed(INVALID_VALUE)) => {
                log::warn!(
                    "Attempt to update local storage with invalid value {:?}",
                    last_submission
                );
            },
            Err(MutateStorageError::ConcurrentModification(_)) => {
                log::error!(
                    "💔 Error updating local storage with latest submission: {:?}",
                    last_submission
                );
            },
            _ => {},
        }
    }
}

impl<T: Config> FinalisedBlockChecker<T::BlockNumber> for Pallet<T> {
    fn is_finalised(block_number: T::BlockNumber) -> bool {
        return Self::latest_finalised_block_number() >= block_number
    }
}

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Debug, Eq, TypeInfo, MaxEncodedLen)]
pub struct SubmissionData<BlockNumber: Member + AtLeast32Bit> {
    pub finalised_block: BlockNumber,
    pub submitted_at_block: BlockNumber,
}

impl<BlockNumber: Member + AtLeast32Bit> SubmissionData<BlockNumber> {
    fn new(finalised_block: BlockNumber, submitted_at_block: BlockNumber) -> Self {
        return SubmissionData::<BlockNumber> { finalised_block, submitted_at_block }
    }
}
