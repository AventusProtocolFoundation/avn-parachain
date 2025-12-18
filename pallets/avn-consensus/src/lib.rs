#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub mod default_weights;
pub use default_weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
    use super::WeightInfo;

    use frame_support::{pallet_prelude::*, traits::ConstU32};
    use frame_system::{
        offchain::{SendTransactionTypes, SubmitTransaction},
        pallet_prelude::*,
    };

    use pallet_avn::{self as avn, Error as avn_error};
    use sp_avn_common::event_types::Validator;
    use sp_runtime::{
        traits::{Hash as HashT, Saturating},
        DispatchError, RuntimeAppPublic,
    };
    use sp_std::vec::Vec;

    pub type AVN<T> = avn::Pallet<T>;
    pub type FeedId = u32;

    const PALLET_NAME: &'static [u8] = b"AvnConsensus";

    pub const SUBMIT_CONSENSUS_CONTEXT: &'static [u8] = b"consensus_submit_context";
    pub const CLEAR_CONSENSUS_CONTEXT: &'static [u8] = b"consensus_clear_context";

    pub const MAX_PAYLOAD_LEN: u32 = 4096;
    pub type Payload = BoundedVec<u8, ConstU32<MAX_PAYLOAD_LEN>>;

    /// Bound number of active feeds to keep storage + OCW work bounded.
    pub const MAX_FEEDS: u32 = 32;
    pub type Feeds = BoundedVec<FeedId, ConstU32<MAX_FEEDS>>;

    pub trait OnConsensusReached<T: Config> {
        fn on_consensus(feed_id: FeedId, payload: Vec<u8>, round_id: u32) -> DispatchResult;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// Active feeds that currently need OCW attention (auto-managed):
    #[pallet::storage]
    #[pallet::getter(fn known_feeds)]
    pub type KnownFeeds<T> = StorageValue<_, Feeds, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn round_id)]
    pub type RoundId<T> = StorageMap<_, Blake2_128Concat, FeedId, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn reporters)]
    pub type Reporters<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        (FeedId, u32),
        Blake2_128Concat,
        T::AccountId,
        (),
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn vote_counts)]
    pub type VoteCounts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        (FeedId, u32),
        Blake2_128Concat,
        T::Hash,
        u32,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn payload_by_hash)]
    pub type PayloadByHash<T: Config> =
        StorageMap<_, Blake2_128Concat, (FeedId, T::Hash), Payload, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_submission_block)]
    pub type LastSubmissionBlock<T: Config> =
        StorageMap<_, Blake2_128Concat, FeedId, BlockNumberFor<T>, ValueQuery>;

    #[pallet::config]
    pub trait Config:
        SendTransactionTypes<Call<Self>> + frame_system::Config + pallet_avn::Config
    {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type WeightInfo: WeightInfo;

        #[pallet::constant]
        type RefreshRangeBlocks: Get<u32>;

        #[pallet::constant]
        type ConsensusGracePeriod: Get<u32>;

        type OnConsensusReached: OnConsensusReached<Self>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ConsensusReached { feed_id: FeedId, round_id: u32, payload_hash: T::Hash },
        ConsensusCleared { feed_id: FeedId, round_id: u32 },
    }

    #[pallet::error]
    pub enum Error<T> {
        SubmitterNotAValidator,
        ValidatorAlreadySubmitted,
        MissingPayload,
        GracePeriodNotPassed,
        PayloadTooLarge,
        TooManyFeeds,
        ErrorSigning,
        ErrorSubmittingTransaction,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::submit())]
        pub fn submit(
            origin: OriginFor<T>,
            feed_id: FeedId,
            payload: Vec<u8>,
            submitter: Validator<T::AuthorityId, T::AccountId>,
            _signature: <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            ensure!(
                AVN::<T>::is_validator(&submitter.account_id),
                Error::<T>::SubmitterNotAValidator
            );

            // mark this feed as "active" (OCW will watch it until it resolves)
            Self::activate_feed(feed_id)?;

            let bounded_payload: Payload = payload
                .clone()
                .try_into()
                .map_err(|_| DispatchError::from(Error::<T>::PayloadTooLarge))?;

            let round_id = RoundId::<T>::get(feed_id);

            ensure!(
                !Reporters::<T>::contains_key((feed_id, round_id), &submitter.account_id),
                Error::<T>::ValidatorAlreadySubmitted
            );
            Reporters::<T>::insert((feed_id, round_id), &submitter.account_id, ());

            let payload_hash = T::Hashing::hash(&payload);

            PayloadByHash::<T>::mutate((feed_id, payload_hash), |v| {
                if v.is_none() {
                    *v = Some(bounded_payload.clone());
                }
            });

            let count = VoteCounts::<T>::mutate((feed_id, round_id), payload_hash, |c| {
                *c = c.saturating_add(1);
                *c
            });

            if count > AVN::<T>::quorum() {
                let stored = PayloadByHash::<T>::get((feed_id, payload_hash))
                    .ok_or(Error::<T>::MissingPayload)?;

                T::OnConsensusReached::on_consensus(feed_id, stored.to_vec(), round_id)?;

                LastSubmissionBlock::<T>::insert(
                    feed_id,
                    <frame_system::Pallet<T>>::block_number(),
                );

                RoundId::<T>::insert(feed_id, round_id.saturating_add(1));

                Self::deactivate_feed(feed_id);

                Self::deposit_event(Event::<T>::ConsensusReached {
                    feed_id,
                    round_id,
                    payload_hash,
                });
            }

            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::clear_consensus())]
        pub fn clear_consensus(
            origin: OriginFor<T>,
            feed_id: FeedId,
            submitter: Validator<T::AuthorityId, T::AccountId>,
            _signature: <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            ensure!(
                AVN::<T>::is_validator(&submitter.account_id),
                Error::<T>::SubmitterNotAValidator
            );

            let current_block = <frame_system::Pallet<T>>::block_number();
            let last = LastSubmissionBlock::<T>::get(feed_id);

            let required = last
                .saturating_add(BlockNumberFor::<T>::from(T::RefreshRangeBlocks::get()))
                .saturating_add(BlockNumberFor::<T>::from(T::ConsensusGracePeriod::get()));

            ensure!(current_block >= required, Error::<T>::GracePeriodNotPassed);

            // set last submission back so next cycle can start
            let new_last = current_block
                .saturating_sub(BlockNumberFor::<T>::from(T::RefreshRangeBlocks::get()));
            LastSubmissionBlock::<T>::insert(feed_id, new_last);

            let cleared = RoundId::<T>::get(feed_id);
            RoundId::<T>::insert(feed_id, cleared.saturating_add(1));

            // feed is now unblocked; stop OCW watching until next submit
            Self::deactivate_feed(feed_id);

            Self::deposit_event(Event::<T>::ConsensusCleared { feed_id, round_id: cleared });

            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn offchain_worker(block_number: BlockNumberFor<T>) {
            let setup_result = AVN::<T>::pre_run_setup(block_number, PALLET_NAME.to_vec());
            if let Err(e) = setup_result {
                match e {
                    _ if e == DispatchError::from(avn_error::<T>::OffchainWorkerAlreadyRun) => (),
                    _ => (),
                }
                return;
            }

            let (this_validator, _) = setup_result.expect("validator available from pre_run_setup");

            for feed_id in KnownFeeds::<T>::get().into_iter() {
                let _ = Self::clear_consensus_if_required(feed_id, &this_validator, block_number);
            }
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match source {
                TransactionSource::Local | TransactionSource::InBlock => { /* ok */ },
                _ => return InvalidTransaction::Call.into(),
            }

            match call {
                Call::submit { feed_id, payload, submitter, signature } => {
                    if payload.len() > MAX_PAYLOAD_LEN as usize {
                        return InvalidTransaction::Custom(10u8).into();
                    }

                    let round_id = RoundId::<T>::get(*feed_id);
                    let payload_hash = T::Hashing::hash(payload);

                    let signed = (SUBMIT_CONSENSUS_CONTEXT, feed_id, payload_hash, round_id);

                    if AVN::<T>::signature_is_valid(&signed, submitter, signature) {
                        ValidTransaction::with_tag_prefix("ConsensusSubmit")
                            .and_provides(vec![(
                                SUBMIT_CONSENSUS_CONTEXT,
                                feed_id,
                                round_id,
                                submitter.account_id.clone(),
                            )
                                .encode()])
                            .priority(TransactionPriority::max_value())
                            .longevity(64_u64)
                            .propagate(false)
                            .build()
                    } else {
                        InvalidTransaction::Custom(1u8).into()
                    }
                },

                Call::clear_consensus { feed_id, submitter, signature } => {
                    let round_id = RoundId::<T>::get(*feed_id);
                    let signed = (CLEAR_CONSENSUS_CONTEXT, feed_id, round_id);

                    if AVN::<T>::signature_is_valid(&signed, submitter, signature) {
                        ValidTransaction::with_tag_prefix("ConsensusClear")
                            .and_provides(vec![(
                                CLEAR_CONSENSUS_CONTEXT,
                                feed_id,
                                round_id,
                                submitter.account_id.clone(),
                            )
                                .encode()])
                            .priority(TransactionPriority::max_value())
                            .longevity(64_u64)
                            .propagate(false)
                            .build()
                    } else {
                        InvalidTransaction::Custom(2u8).into()
                    }
                },

                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    impl<T: Config> Pallet<T> {
        fn activate_feed(feed_id: FeedId) -> Result<(), DispatchError> {
            KnownFeeds::<T>::try_mutate(|feeds| -> Result<(), DispatchError> {
                if feeds.iter().any(|x| *x == feed_id) {
                    return Ok(());
                }
                feeds.try_push(feed_id).map_err(|_| Error::<T>::TooManyFeeds)?;
                Ok(())
            })
        }

        fn deactivate_feed(feed_id: FeedId) {
            KnownFeeds::<T>::mutate(|feeds| {
                feeds.retain(|x| *x != feed_id);
            });
        }

        fn can_clear(feed_id: FeedId, current: BlockNumberFor<T>) -> bool {
            let last = LastSubmissionBlock::<T>::get(feed_id);

            current >=
                last.saturating_add(BlockNumberFor::<T>::from(T::RefreshRangeBlocks::get()))
                    .saturating_add(BlockNumberFor::<T>::from(T::ConsensusGracePeriod::get()))
        }

        fn clear_consensus_if_required(
            feed_id: FeedId,
            submitter: &Validator<T::AuthorityId, T::AccountId>,
            current_block: BlockNumberFor<T>,
        ) -> Result<(), DispatchError> {
            if !Self::can_clear(feed_id, current_block) {
                return Ok(());
            }

            let round_id = RoundId::<T>::get(feed_id);

            let guard_lock_name = Self::create_guard_lock(
                b"clear_consensus::",
                (feed_id, round_id),
                &submitter.account_id,
            );

            let mut lock = AVN::<T>::get_ocw_locker(&guard_lock_name);
            if let Ok(guard) = lock.try_lock() {
                let signature = submitter
                    .key
                    .sign(&(CLEAR_CONSENSUS_CONTEXT, feed_id, round_id).encode())
                    .ok_or(Error::<T>::ErrorSigning)?;

                SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                    Call::clear_consensus { feed_id, submitter: submitter.clone(), signature }
                        .into(),
                )
                .map_err(|_| DispatchError::from(Error::<T>::ErrorSubmittingTransaction))?;

                guard.forget();
            }

            Ok(())
        }

        pub fn create_guard_lock<Id: Encode>(
            prefix: &'static [u8],
            id: Id,
            who: &T::AccountId,
        ) -> Vec<u8> {
            let mut name = prefix.to_vec();
            name.extend_from_slice(&id.encode());
            name.extend_from_slice(&who.encode());
            name
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
