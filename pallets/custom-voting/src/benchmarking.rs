#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_system::RawOrigin;
use sp_runtime::traits::Bounded;

const SEED: u32 = 0;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
    frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

benchmarks! {
    submit_ethereum_vote {
        let caller: T::AccountId = whitelisted_caller();
        let poll_index = PollIndexOf::<T>::max_value();
        let vote = AccountVote::Standard { vote: true, balance: BalanceOf::<T>::max_value() };
        
        let ethereum_signature = [0u8; 65];
        let now = T::TimeProvider::now();
        
        let vote_proof = VoteProof {
            voter: caller.clone(),
            vote: vote.clone(),
            timestamp: now,
            ethereum_signature,
        };

        // Set up an ongoing poll
        let tally = T::Tally::new(true);  // Assuming `new` method exists, adjust as necessary
        <T as VotingConfig>::Polls::create_ongoing(poll_index, tally).unwrap();

    }: _(RawOrigin::Signed(caller.clone()), poll_index, vote_proof)
    verify {
        assert_last_event::<T>(Event::EthereumVoteProcessed(caller, poll_index).into());
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}