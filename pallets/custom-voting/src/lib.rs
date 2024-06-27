#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::{Currency, Polling};
pub use pallet::*;
pub use pallet_conviction_voting::{Config as VotingConfig, TallyOf};
pub mod default_weights;
use sp_std::vec::Vec;

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

// const PALLET_NAME: &'static [u8] = b"CustomVoting";
const ETHEREUM_VOTE: &'static [u8] = b"EthereumVote";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::{default_weights::WeightInfo, BalanceOf, PollIndexOf, VotingConfig, ETHEREUM_VOTE};
    use codec::{Decode, Encode};
    use core::fmt::Debug;
    use frame_support::{
        crypto::ecdsa,
        pallet_prelude::*,
        traits::{Polling, Time},
    };
    use frame_system::pallet_prelude::*;
    use pallet_conviction_voting::AccountVote;
    use scale_info::TypeInfo;
    use sp_avn_common::recover_public_key_from_ecdsa_signature;
    use sp_core::ecdsa::Signature as EcdsaSignature;
    use sp_io::hashing::keccak_256;
    use sp_runtime::{
        traits::{AtLeast32Bit, Zero},
        ArithmeticError,
    };

    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
    #[scale_info(skip_type_params(T))]
    pub struct VoteProof<T: Config> {
        pub voter: T::AccountId,
        pub vote: AccountVote<BalanceOf<T>>,
        pub timestamp: T::Moment,
        pub ethereum_signature: EcdsaSignature,
        pub ethereum_public_key: T::EthereumPublicKey,
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
            + Debug
            + Encode;
        type EthereumPublicKey: AsRef<[u8]> + Parameter;
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
        InvalidEthereumSignature,
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
            // BOTH NEEDED
            // 1. validate eth signature
            // Construct the message that was signed
            let message =
                Self::construct_vote_message(poll_index, &vote_proof.vote, vote_proof.timestamp);

            ensure!(
                Self::eth_signature_is_valid(
                    message,
                    &vote_proof.ethereum_public_key,
                    &vote_proof.ethereum_signature,
                ),
                Error::<T>::InvalidEthereumSignature
            );

            // 2. can you think of a way to extend avn-proxy to work with ecdsa signature
            // avn-proxy does the validation of the signature and passes the transaction to this
            // pallet
            // signer is going to be the extracted avn address
            Self::do_vote(vote_proof.voter.clone(), poll_index, vote_proof.vote)?;

            ProcessedVotes::<T>::insert(&vote_proof.voter, &poll_index, true);

            Self::deposit_event(Event::EthereumVoteProcessed(vote_proof.voter, poll_index));
            Ok(())
        }
    }

    impl<T: Config> Pallet<T>
    where
        VoteProof<T>: Encode + Decode + Debug,
    {
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

        fn construct_vote_message(
            poll_index: PollIndexOf<T>,
            vote: &AccountVote<BalanceOf<T>>,
            timestamp: <<T as Config>::TimeProvider as Time>::Moment,
        ) -> Vec<u8> {
            let vote_type_hash = keccak_256(b"Vote(uint256 pollIndex,int8 voteType,uint256 aye,uint256 nay,uint256 abstain,uint256 timestamp)");

            let (vote_type, aye, nay, abstain) = match vote {
                AccountVote::Standard { vote, balance } => (
                    1i8,
                    if vote.aye { *balance } else { Zero::zero() },
                    if !vote.aye { *balance } else { Zero::zero() },
                    Zero::zero(),
                ),
                AccountVote::Split { aye, nay } => (2i8, *aye, *nay, Zero::zero()),
                AccountVote::SplitAbstain { aye, nay, abstain } => (3i8, *aye, *nay, *abstain),
            };

            let vote_data = (poll_index, vote_type, aye, nay, abstain, timestamp).encode();

            let vote_hash =
                keccak_256(&[vote_type_hash.to_vec(), keccak_256(&vote_data).to_vec()].concat());

            let message = [ETHEREUM_VOTE, &vote_hash].concat();

            keccak_256(&message).to_vec()
        }

        fn eth_signature_is_valid(
            data: Vec<u8>,
            public_key: &T::EthereumPublicKey,
            signature: &EcdsaSignature,
        ) -> bool {
            let recovered_public_key =
                recover_public_key_from_ecdsa_signature(signature.clone(), hex::encode(data));
            if let Ok(recovered_key) = recovered_public_key {
                recovered_key.as_ref() == public_key.as_ref()
            } else {
                false
            }
        }
    }
}
