#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::{Polling, Currency};
pub use pallet::*;
pub use pallet_conviction_voting::{Config as VotingConfig, TallyOf};
pub mod default_weights;

// #[cfg(feature = "runtime-benchmarks")]
// mod benchmarking;

pub type PollIndexOf<T, I = ()> = <<T as VotingConfig<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
pub type BalanceOf<T, I = ()> =
    <<T as VotingConfig<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        pallet_prelude::*,
        traits::Polling,
    };
    use frame_system::pallet_prelude::*;
    use pallet_conviction_voting::AccountVote;
    use crate::default_weights::WeightInfo;
    use crate::{PollIndexOf, BalanceOf, VotingConfig};
    use sp_runtime::ArithmeticError;

    #[pallet::config]
    pub trait Config: frame_system::Config + VotingConfig {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        // type Balance: Parameter + From<u64> + Into<u128> + Copy;
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
    }

    #[pallet::error]
    pub enum Error<T> {
        NoneValue,
        StorageOverflow,
        MaxVotesReached,
        AlreadyDelegating,
        NotOngoing,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::custom_vote_weight())]
        pub fn vote(
            origin: OriginFor<T>,
            poll_index: PollIndexOf<T>,
            vote: AccountVote<BalanceOf<T>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin.clone())?;

            <T as VotingConfig>::Polls::try_access_poll(poll_index, |poll_status| {
                let (tally, class) = poll_status.ensure_ongoing().ok_or(Error::<T>::NotOngoing)?;
                pallet_conviction_voting::VotingFor::<T, ()>::try_mutate(who, &class, |voting| {
                    if let pallet_conviction_voting::Voting::Casting(pallet_conviction_voting::Casting { ref mut votes, delegations, .. }) = voting {
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
                        return Err(Error::<T>::AlreadyDelegating.into());
                    }
                    Ok(())
                })
            })
        }
    }
}

// #[cfg(test)]
// mod mock;

// #[cfg(test)]
// #[path = "tests/tests.rs"]
// mod tests;