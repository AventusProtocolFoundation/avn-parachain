#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as AvnConsensus;

use codec::{Decode, Encode};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::{
    assert_ok,
    pallet_prelude::Weight,
    traits::{Get, Hooks},
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use scale_info::prelude::{format, vec, vec::Vec};
use sp_avn_common::event_types::Validator;
use sp_runtime::{RuntimeAppPublic, SaturatedConversion, WeakBoundedVec};

fn generate_validators<T: Config>(count: usize) -> Vec<Validator<T::AuthorityId, T::AccountId>> {
    let mut validators = Vec::new();

    for i in 0..=count {
        let seed = format!("//benchmark_{}", i).as_bytes().to_vec();
        let authority_id = T::AuthorityId::generate_pair(Some(seed));

        // Create dummy AccountId
        let account_seed = [i as u8; 32];
        let account_id = T::AccountId::decode(&mut &account_seed[..])
            .unwrap_or_else(|_| panic!("Failed to create AccountId from seed for validator {}", i));

        validators.push(Validator { key: authority_id, account_id });
    }

    pallet_avn::Validators::<T>::put(WeakBoundedVec::force_from(
        validators.clone(),
        Some("Failed to set validators"),
    ));

    validators
}

benchmarks! {
    submit {
        // Create N validators
        let current_authors = generate_validators::<T>(10);

        let feed_id: u32 = 1;

        let payload = b"benchmark-payload".to_vec();

        let round_id = RoundId::<T>::get(feed_id);

        let context = (SUBMIT_CONSENSUS_CONTEXT, feed_id, payload.clone(), round_id).encode();

        let quorum = AVN::<T>::quorum() as usize;

        // Pre-submit quorum votes so the measured call will be the one that triggers consensus
        for i in 0..quorum {
            let signature = current_authors[i].key.sign(&context).expect("Valid signature");
            AvnConsensus::<T>::submit(
                RawOrigin::None.into(),
                feed_id,
                payload.clone(),
                current_authors[i].clone(),
                signature,
            )?;
        }

        // This call should trigger consensus
        let signature = current_authors[quorum].key.sign(&context).expect("Valid signature");
    }: _(RawOrigin::None, feed_id, payload.clone(), current_authors[quorum].clone(), signature)
    verify {
        // All validators up to quorum submitted for this round
        for i in 0..=quorum {
            assert!(
                Reporters::<T>::contains_key((feed_id, round_id), &current_authors[i].account_id),
                "expected reporter to be recorded for validator {}",
                i
            );
        }

        // Consensus reached => round bumped
        assert_eq!(RoundId::<T>::get(feed_id), round_id + 1);

        // Consensus reached => feed should be removed from KnownFeeds
        let feeds = KnownFeeds::<T>::get();
        assert!(!feeds.iter().any(|f| *f == feed_id));
    }

    clear_consensus {
        let validator = generate_validators::<T>(1)[0].clone();

        let feed_id: u32 = 2;
        let round_id = RoundId::<T>::get(feed_id);

        // Ensure feed is known/active
        let payload = b"partial".to_vec();
        let submit_ctx = (SUBMIT_CONSENSUS_CONTEXT, feed_id, payload.clone(), round_id).encode();
        let submit_sig = validator.key.sign(&submit_ctx).expect("Valid signature");

        AvnConsensus::<T>::submit(
            RawOrigin::None.into(),
            feed_id,
            payload,
            validator.clone(),
            submit_sig,
        )?;

        let current_round = RoundId::<T>::get(feed_id);
        let clear_ctx = (CLEAR_CONSENSUS_CONTEXT, feed_id, current_round).encode();
        let clear_sig = validator.key.sign(&clear_ctx).expect("Valid signature");

        // Jump to a block where grace period has passed
        let last_submission = LastSubmissionBlock::<T>::get(feed_id);
        let required_block_u32 =
            last_submission.saturated_into::<u32>()
                .saturating_add(T::RefreshRangeBlocks::get())
                .saturating_add(T::ConsensusGracePeriod::get())
                .saturating_add(1);

        let now = BlockNumberFor::<T>::from(required_block_u32);
        frame_system::Pallet::<T>::set_block_number(now);

        let old_round = RoundId::<T>::get(feed_id);
    }: _(RawOrigin::None, feed_id, validator.clone(), clear_sig)
    verify {
        // Round bumped
        assert_eq!(RoundId::<T>::get(feed_id), old_round + 1);

        // Feed removed from KnownFeeds on unblock
        let feeds = KnownFeeds::<T>::get();
        assert!(!feeds.iter().any(|f| *f == feed_id));
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().with_validators().as_externality(),
    crate::mock::TestRuntime,
);
