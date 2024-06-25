#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::{Currency, Polling};
pub use pallet::*;
pub use pallet_conviction_voting::{Config as VotingConfig, TallyOf};
pub mod default_weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

pub type PollIndexOf<T, I = ()> = <<T as VotingConfig<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
pub type BalanceOf<T, I = ()> =
    <<T as VotingConfig<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use crate::{default_weights::WeightInfo, BalanceOf, PollIndexOf, VotingConfig};
    use codec::{Decode, Encode};
    use core::fmt::Debug;
    use frame_support::{
        pallet_prelude::*,
        traits::{Polling, Time},
    };
    use frame_system::pallet_prelude::*;
    use pallet_conviction_voting::AccountVote;
    use scale_info::TypeInfo;
    use sp_runtime::{traits::AtLeast32Bit, ArithmeticError};

    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
    #[scale_info(skip_type_params(T))]
    pub struct VoteProof<T: Config> {
        voter: T::AccountId,
        vote: AccountVote<BalanceOf<T>>,
        timestamp: T::Moment,
        ethereum_signature: [u8; 65],
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + VotingConfig {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
        type TimeProvider: Time<Moment = Self::Moment>;
        type MaxVoteAge: Get<Self::Moment>;
        type Moment: Clone
            + Copy
            + PartialOrd
            + AtLeast32Bit
            + Default
            + From<u64>
            + TypeInfo
            + Debug;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn processed_votes)]
    pub type ProcessedVotes<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        T::AccountId,
        Twox64Concat,
        PollIndexOf<T>,
        bool,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        VoteRecorded(T::AccountId, PollIndexOf<T>),
        EthereumVoteProcessed(T::AccountId, PollIndexOf<T>),
    }

    #[pallet::error]
    pub enum Error<T> {
        MaxVotesReached,
        AlreadyDelegating,
        NotOngoing,
        AlreadyVoted,
        FutureTimestamp,
        VoteTooOld,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        VoteProof<T>: Encode + Decode + Debug,
    {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::vote())]
        pub fn vote(
            origin: OriginFor<T>,
            poll_index: PollIndexOf<T>,
            vote: AccountVote<BalanceOf<T>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_vote(who, poll_index, vote)
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::submit_ethereum_vote())]
        pub fn submit_ethereum_vote(
            origin: OriginFor<T>,
            poll_index: PollIndexOf<T>,
            vote_proof: VoteProof<T>,
        ) -> DispatchResult {
            let _ = ensure_signed(origin)?;

            ensure!(
                !ProcessedVotes::<T>::contains_key(&vote_proof.voter, &poll_index),
                Error::<T>::AlreadyVoted
            );

            let now = T::TimeProvider::now();
            ensure!(vote_proof.timestamp <= now, Error::<T>::FutureTimestamp);
            ensure!(now - vote_proof.timestamp <= T::MaxVoteAge::get(), Error::<T>::VoteTooOld);

            Self::do_vote(vote_proof.voter.clone(), poll_index, vote_proof.vote)?;

            ProcessedVotes::<T>::insert(&vote_proof.voter, &poll_index, true);

            Self::deposit_event(Event::EthereumVoteProcessed(vote_proof.voter, poll_index));
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn do_vote(
            who: T::AccountId,
            poll_index: PollIndexOf<T>,
            vote: AccountVote<BalanceOf<T>>,
        ) -> DispatchResult {
            <T as VotingConfig>::Polls::try_access_poll(poll_index, |poll_status| {
                let (tally, class) = poll_status.ensure_ongoing().ok_or(Error::<T>::NotOngoing)?;
                pallet_conviction_voting::VotingFor::<T, ()>::try_mutate(
                    who.clone(),
                    &class,
                    |voting| {
                        if let pallet_conviction_voting::Voting::Casting(
                            pallet_conviction_voting::Casting {
                                ref mut votes, delegations, ..
                            },
                        ) = voting
                        {
                            match votes.binary_search_by_key(&poll_index, |i| i.0) {
                                Ok(i) => {
                                    tally.remove(votes[i].1).ok_or(ArithmeticError::Underflow)?;
                                    if let Some(approve) = votes[i].1.as_standard() {
                                        tally.reduce(approve, *delegations);
                                    }
                                    votes[i].1 = vote;
                                },
                                Err(i) => {
                                    votes
                                        .try_insert(i, (poll_index, vote))
                                        .map_err(|_| Error::<T>::MaxVotesReached)?;
                                },
                            }
                            tally.add(vote).ok_or(ArithmeticError::Overflow)?;
                            if let Some(approve) = vote.as_standard() {
                                tally.increase(approve, *delegations);
                            }
                        } else {
                            return Err(Error::<T>::AlreadyDelegating.into())
                        }
                        Ok(())
                    },
                )
            })?;

            Self::deposit_event(Event::VoteRecorded(who, poll_index));
            Ok(())
        }
    }
}