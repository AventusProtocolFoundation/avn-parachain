//! # custom voting pallet
// Copyright 2024 Aventus Systems (UK) Ltd.

//! custom voting pallet benchmarking.

// #![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use pallet_conviction_voting::AccountVote;

use crate::Pallet as CustomVotingPallet;

benchmarks! {
    vote {
        let poll_index = 0;
        let vote = AccountVote::Standard(100);
        let caller = <T as frame_system::Config>::AccountId::from([1; 20]);
        let poll_status = <T as VotingConfig>::Polls::poll_status(poll_index).unwrap();
        let tally = poll_status.tally;
        let class = poll_status.class;
        let delegations = poll_status.delegations;
        let voting = pallet_conviction_voting::Voting::Casting(pallet_conviction_voting::Casting {
            votes: vec![(poll_index, vote.clone())],
            delegations,
        });
        pallet_conviction_voting::VotingFor::<T, ()>::insert(caller, &class, &voting);

    }: _(RawOrigin::Signed(caller), poll_index, vote)
    verify {
        let poll_status = <T as VotingConfig>::Polls::poll_status(poll_index).unwrap();
        assert!(poll_status.tally.contains_key(&caller));
    }
}

impl_benchmark_test_suite!(
    CustomVotingPallet,
    crate::mock::ExtBuilder::build_default().as_externality(),
    crate::mock::TestRuntime,
);
