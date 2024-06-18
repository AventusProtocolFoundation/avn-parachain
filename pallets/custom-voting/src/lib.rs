#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::{Polling, Currency};
pub use pallet::*;
pub use pallet_conviction_voting::{Config as VotingConfig, TallyOf};
pub mod default_weights;
use sp_runtime::traits::StaticLookup;
use log;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub type PollIndexOf<T, I = ()> = <<T as VotingConfig<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
pub type ClassOf<T, I = ()> = <<T as VotingConfig<I>>::Polls as Polling<TallyOf<T, I>>>::Class;
pub type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
pub type BalanceOf<T, I = ()> =
    <<T as VotingConfig<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub type VotesOf<T, I = ()> = BalanceOf<T, I>;
#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        pallet_prelude::*,
        traits::{fungible, Currency, LockableCurrency, Polling, ReservableCurrency},
    };
    use frame_system::pallet_prelude::*;
    use pallet_conviction_voting::{AccountVote, TallyOf};
    use crate::default_weights::WeightInfo;
    use crate::{PollIndexOf, ClassOf, AccountIdLookupOf, BalanceOf, VotingConfig};
    use pallet_conviction_voting::Conviction;
    use sp_runtime::{traits::StaticLookup, ArithmeticError};

    #[pallet::config]
    pub trait Config<I: 'static = ()>: frame_system::Config + VotingConfig<I> {
        type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type Balance: Parameter + From<u64> + Into<u128> + Copy;
        type Currency: ReservableCurrency<Self::AccountId>
            + LockableCurrency<Self::AccountId, Moment = BlockNumberFor<Self>>
            + fungible::Inspect<Self::AccountId>;
        type WeightInfo: WeightInfo;
        type VoteLockingPeriod: Get<BlockNumberFor<Self>>;
        type MaxVotes: Get<u32>;
        type MaxTurnout: Get<u128>;
        type Polls: Polling<
            TallyOf<Self, I>,
            Votes = BalanceOf<Self, I>,
            Moment = BlockNumberFor<Self>,
        >;
    }

    #[pallet::pallet]
    pub struct Pallet<T, I = ()>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config<I>, I: 'static = ()> {
        CustomVoteWeightCalculated(T::AccountId, T::Balance, u128),
    }

    #[pallet::error]
    pub enum Error<T, I = ()> {
        NoneValue,
        StorageOverflow,
        MaxVotesReached,
        AlreadyDelegating,
    }

    // #[pallet::hooks]
    // impl<T: Config, I = ()> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as crate::Config<I>>::WeightInfo::calculate_custom_vote_weight())]
        pub fn vote(
            origin: OriginFor<T>,
            poll_index: PollIndexOf<T, I>,
            vote: AccountVote<BalanceOf<T, I>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin.clone())?;

            <T as VotingConfig<I>>::Polls::try_access_poll(poll_index, |poll_status| {
                let (tally, class) = poll_status.ensure_ongoing().ok_or(pallet_conviction_voting::Error::<T, ()>::NotOngoing)?;
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
