#![cfg(test)]

use crate::{pallet as consensus, Error};
use codec::Encode;
use frame_support::{assert_noop, assert_ok};

use super::mock::*;

use sp_avn_common::event_types::Validator;
use sp_runtime::{
    testing::{TestSignature, UintAuthorityId},
    traits::Saturating,
};

const FEED_ID_1: u32 = 1;
const FEED_ID_2: u32 = 2;

fn submit_sig(
    feed_id: u32,
    payload: &Vec<u8>,
    submitter: &Validator<UintAuthorityId, AccountId>,
) -> TestSignature {
    let round_id = consensus::RoundId::<TestRuntime>::get(feed_id);

    let signing_payload =
        (crate::pallet::SUBMIT_CONSENSUS_CONTEXT, feed_id, payload, round_id).encode();
    generate_signature(submitter, signing_payload.as_slice())
}

fn clear_sig(feed_id: u32, submitter: &Validator<UintAuthorityId, AccountId>) -> TestSignature {
    let round_id = consensus::RoundId::<TestRuntime>::get(feed_id);

    let signing_payload = (crate::pallet::CLEAR_CONSENSUS_CONTEXT, feed_id, round_id).encode();
    generate_signature(submitter, signing_payload.as_slice())
}

fn submit(feed_id: u32, payload: Vec<u8>, author_id: u64) {
    let submitter = create_validator(author_id);
    let sig = submit_sig(feed_id, &payload, &submitter);

    assert_ok!(AvnConsensus::submit(RuntimeOrigin::none(), feed_id, payload, submitter, sig,));
}

fn clear_consensus(feed_id: u32, author_id: u64) {
    let submitter = create_validator(author_id);
    let sig = clear_sig(feed_id, &submitter);

    assert_ok!(AvnConsensus::clear_consensus(RuntimeOrigin::none(), feed_id, submitter, sig,));
}

#[test]
fn submit_adds_feed_to_knownfeeds() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        clear_router_log();
        assert!(consensus::KnownFeeds::<TestRuntime>::get().is_empty());

        submit(FEED_ID_1, b"hello".to_vec(), 1);

        let feeds = consensus::KnownFeeds::<TestRuntime>::get();
        assert!(feeds.iter().any(|f| *f == FEED_ID_1));
    });
}

#[test]
fn knownfeeds_does_not_duplicate_same_feed() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        clear_router_log();

        submit(FEED_ID_1, b"a".to_vec(), 1);
        submit(FEED_ID_1, b"a".to_vec(), 2);

        let feeds = consensus::KnownFeeds::<TestRuntime>::get();
        let occurrences = feeds.iter().filter(|f| **f == FEED_ID_1).count();
        assert_eq!(occurrences, 1);
    });
}

#[test]
fn same_validator_cannot_submit_twice_in_same_round() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        let payload = b"x".to_vec();
        let submitter = create_validator(1);

        let sig1 = submit_sig(FEED_ID_1, &payload, &submitter);
        assert_ok!(AvnConsensus::submit(
            RuntimeOrigin::none(),
            FEED_ID_1,
            payload.clone(),
            submitter.clone(),
            sig1,
        ));

        let sig2 = submit_sig(FEED_ID_1, &payload, &submitter);
        assert_noop!(
            AvnConsensus::submit(RuntimeOrigin::none(), FEED_ID_1, payload, submitter, sig2,),
            Error::<TestRuntime>::ValidatorAlreadySubmitted
        );
    });
}

#[test]
fn payload_too_large_is_rejected() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        let submitter = create_validator(1);

        let too_big = vec![0u8; (crate::MAX_PAYLOAD_LEN as usize) + 1];
        let sig = submit_sig(FEED_ID_1, &too_big, &submitter);

        assert_noop!(
            AvnConsensus::submit(RuntimeOrigin::none(), FEED_ID_1, too_big, submitter, sig,),
            Error::<TestRuntime>::PayloadTooLarge
        );
    });
}

#[test]
fn consensus_reached_calls_router_and_removes_feed_and_bumps_round() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        clear_router_log();

        let payload = b"same-payload".to_vec();
        let start_round = consensus::RoundId::<TestRuntime>::get(FEED_ID_1);

        let required = AVN::quorum().saturating_add(1);

        assert!(required <= 10, "mock provides 10 validators; required={}", required);

        for id in 1u64..=(required as u64) {
            submit(FEED_ID_1, payload.clone(), id);
        }

        assert_eq!(router_log_len(), 1);

        let (feed, p, round) = router_log_get(0);
        assert_eq!(feed, FEED_ID_1);
        assert_eq!(p, payload);
        assert_eq!(round, start_round);

        let feeds = consensus::KnownFeeds::<TestRuntime>::get();
        assert!(!feeds.iter().any(|f| *f == FEED_ID_1));

        assert_eq!(consensus::RoundId::<TestRuntime>::get(FEED_ID_1), start_round + 1);
    });
}

#[test]
fn clear_consensus_before_grace_period_fails() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        clear_router_log();

        submit(FEED_ID_2, b"initial".to_vec(), 1);

        let now = System::block_number();
        consensus::LastSubmissionBlock::<TestRuntime>::insert(FEED_ID_2, now);

        let submitter = create_validator(1);
        let sig = clear_sig(FEED_ID_2, &submitter);

        assert_noop!(
            AvnConsensus::clear_consensus(RuntimeOrigin::none(), FEED_ID_2, submitter, sig),
            Error::<TestRuntime>::GracePeriodNotPassed
        );
    });
}

#[test]
fn clear_consensus_after_grace_period_bumps_round_and_removes_feed() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        clear_router_log();

        submit(FEED_ID_2, b"initial".to_vec(), 1);

        let old_round = consensus::RoundId::<TestRuntime>::get(FEED_ID_2);

        let last = consensus::LastSubmissionBlock::<TestRuntime>::get(FEED_ID_2);

        let required_block = last
            .saturating_add(RefreshRangeBlocks::get().into())
            .saturating_add(ConsensusGracePeriod::get().into());

        System::set_block_number(required_block);

        clear_consensus(FEED_ID_2, 1);

        assert_eq!(consensus::RoundId::<TestRuntime>::get(FEED_ID_2), old_round + 1);

        let feeds = consensus::KnownFeeds::<TestRuntime>::get();
        assert!(!feeds.iter().any(|f| *f == FEED_ID_2));
    });
}

#[test]
fn different_payloads_do_not_combine_towards_quorum() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        clear_router_log();

        let payload_a = b"payload-a".to_vec();
        let payload_b = b"payload-b".to_vec();

        let required = AVN::quorum().saturating_add(1);
        assert!(required <= 10, "mock provides 10 validators; required={}", required);

        // Submit (required - 1) votes for payload A
        for id in 1u64..=((required as u64).saturating_sub(1)) {
            submit(FEED_ID_1, payload_a.clone(), id);
        }

        // One vote for payload B (should not help A)
        submit(FEED_ID_1, payload_b.clone(), required as u64);

        // Not enough yet for A -> no consensus
        assert_eq!(router_log_len(), 0);

        // One more vote for payload A crosses quorum -> consensus
        submit(FEED_ID_1, payload_a.clone(), (required as u64) + 1);

        assert_eq!(router_log_len(), 1);
        let (feed, p, _round) = router_log_get(0);
        assert_eq!(feed, FEED_ID_1);
        assert_eq!(p, payload_a);
    });
}

#[test]
fn after_consensus_new_round_accepts_submissions_again() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        clear_router_log();

        let payload = b"same-payload".to_vec();
        let required = AVN::quorum().saturating_add(1);
        assert!(required <= 10, "mock provides 10 validators; required={}", required);

        // Reach consensus for round 0
        for id in 1u64..=(required as u64) {
            submit(FEED_ID_1, payload.clone(), id);
        }

        assert_eq!(router_log_len(), 1);
        assert_eq!(consensus::RoundId::<TestRuntime>::get(FEED_ID_1), 1);

        // Submitting again in the new round should work
        submit(FEED_ID_1, payload.clone(), 1);

        // Feed should be re-added to known feeds (since it is "active" again)
        let feeds = consensus::KnownFeeds::<TestRuntime>::get();
        assert!(feeds.iter().any(|f| *f == FEED_ID_1));
    });
}

#[test]
fn clear_consensus_after_grace_period_works_even_if_quorum_never_reached() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        clear_router_log();

        // Activate feed with a single submission (not enough for quorum)
        submit(FEED_ID_2, b"partial".to_vec(), 1);

        let old_round = consensus::RoundId::<TestRuntime>::get(FEED_ID_2);

        let last = consensus::LastSubmissionBlock::<TestRuntime>::get(FEED_ID_2);

        let required_block = last
            .saturating_add(RefreshRangeBlocks::get().into())
            .saturating_add(ConsensusGracePeriod::get().into());

        System::set_block_number(required_block);

        // Clear should bump round + remove feed; router should NOT be called
        clear_consensus(FEED_ID_2, 1);

        assert_eq!(router_log_len(), 0);
        assert_eq!(consensus::RoundId::<TestRuntime>::get(FEED_ID_2), old_round + 1);

        let feeds = consensus::KnownFeeds::<TestRuntime>::get();
        assert!(!feeds.iter().any(|f| *f == FEED_ID_2));
    });
}
