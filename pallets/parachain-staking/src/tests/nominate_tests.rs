//Copyright 2022 Aventus Network Services.

#![cfg(test)]

use crate::{
    assert_eq_events, assert_eq_last_events, assert_event_emitted, assert_last_event,
    assert_tail_eq,
    mock::{
        roll_one_block, roll_to, roll_to_era_begin, roll_to_era_end, set_author, set_reward_pot,
        AccountId, Balances, Event as MetaEvent, ExtBuilder, Origin, ParachainStaking, Test,
        TestAccount, AvnProxy, sign, Signature, Staker, build_proof
    },
    nomination_requests::{CancelledScheduledRequest, NominationAction, ScheduledRequest},
    AtStake, Bond, CollatorStatus, Error, Event, NominationScheduledRequests, NominatorAdded,
    NominatorState, NominatorStatus, NOMINATOR_LOCK_ID, encode_signed_nominate_params, Proof, Config, StaticLookup
};
use crate::mock::Call as MockCall;
use frame_support::{assert_noop, assert_ok};
use sp_runtime::{traits::Zero, DispatchError, ModuleError, Perbill};
use frame_system::{self as system};

fn to_acc_id(id: u64) -> AccountId {
    return TestAccount::new(id).account_id()
}

mod proxy_signed_nominate {
    use super::*;

        fn create_call_for_nominate(
            staker: &Staker,
            sender_nonce: u64,
            targets: Vec<<<Test as system::Config>::Lookup as StaticLookup>::Source>
        ) -> Box<<Test as Config>::Call> {
            let proof = create_proof_for_signed_nominate(sender_nonce, staker, &targets);
            return Box::new(MockCall::ParachainStaking(
                super::super::Call::<Test>::signed_nominate { proof, targets },
            ));
        }

        fn create_proof_for_signed_nominate(
            sender_nonce: u64,
            staker: &Staker,
            targets: &Vec<<<Test as system::Config>::Lookup as StaticLookup>::Source>
        ) -> Proof<Signature, AccountId> {
            let data_to_sign = encode_signed_nominate_params::<Test>(
                staker.relayer.clone(),
                targets,
                sender_nonce,
            );

            let signature = sign(&staker.key_pair, &data_to_sign);
            return build_proof(&staker.account_id, &staker.relayer, signature);
        }

    #[test]
    fn succeeds_with_good_parameters() {
        let collator_1 = to_acc_id(1u64);
        let collator_2 = to_acc_id(2u64);
        let staker: Staker = Default::default();
        let initial_collator_stake = 10;
        let initial_balance = 10000;
        ExtBuilder::default()
            .with_balances(vec![
                (collator_1, initial_balance),
                (collator_2, initial_balance),
                (staker.account_id, initial_balance),
                (staker.relayer, initial_balance)])
            .with_candidates(vec![
                (collator_1, initial_collator_stake),
                (collator_2, initial_collator_stake)])
            .build()
            .execute_with(|| {
                let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                let nominate_call = create_call_for_nominate(&staker, nonce, vec![collator_1, collator_2]);
                assert_ok!(AvnProxy::proxy(Origin::signed(staker.relayer), nominate_call, None));

                let collators = ParachainStaking::selected_candidates();
                let min_user_stake = ParachainStaking::min_total_nominator_stake();
                let expected_total_user_stake = (collators.len() as u128) * min_user_stake;

                // The staker state has also been updated
                let staker_state = ParachainStaking::nominator_state(staker.account_id).unwrap();
                assert_eq!(staker_state.total(), expected_total_user_stake);

                // Each collator has been nominated by the expected amount
                for (index, collator) in collators.into_iter().enumerate() {
                    // We should have one event per collator
                    assert_event_emitted!(Event::Nomination {
                        nominator: staker.account_id,
                        locked_amount: min_user_stake,
                        candidate: collator,
                        nominator_position: NominatorAdded::AddedToTop { new_total: initial_collator_stake + min_user_stake },
                    });

                    // Staker state reflects the new nomination for each collator
                    assert_eq!(staker_state.nominations.0[index].owner, collator);
                    assert_eq!(staker_state.nominations.0[index].amount, min_user_stake);

                    // Collator state has been updated
                    let collator_state = ParachainStaking::candidate_info(collator).unwrap();
                    assert_eq!(collator_state.total_counted, initial_collator_stake + min_user_stake);

                    // Collator nominations have also been updated
                    let top_nominations = ParachainStaking::top_nominations(collator).unwrap();
                    assert_eq!(top_nominations.nominations[0].owner, staker.account_id);
                    assert_eq!(top_nominations.nominations[0].amount, min_user_stake);
                    assert_eq!(top_nominations.total, min_user_stake);
                }

                // The staker free balance has been reduced
                assert_eq!(
                    ParachainStaking::get_nominator_stakable_free_balance(&staker.account_id),
                    10000 - expected_total_user_stake
                );

            })
    }
}

// NOMINATE

mod existing_direct_nominate_tests {
    use super::*;

    #[test]
    fn nominate_event_emits_correctly() {
        let account_id = to_acc_id(1u64);
        ExtBuilder::default()
            .with_balances(vec![(account_id, 30), (to_acc_id(2), 10)])
            .with_candidates(vec![(account_id, 30)])
            .build()
            .execute_with(|| {
                assert_ok!(ParachainStaking::nominate(
                    Origin::signed(to_acc_id(2)),
                    account_id,
                    10,
                    0,
                    0
                ));
                assert_last_event!(MetaEvent::ParachainStaking(Event::Nomination {
                    nominator: to_acc_id(2),
                    locked_amount: 10,
                    candidate: account_id,
                    nominator_position: NominatorAdded::AddedToTop { new_total: 40 },
                }));
            });
    }

    #[test]
    fn nominate_reserves_balance() {
        let account_id = to_acc_id(1u64);
        let account_id_2 = to_acc_id(2u64);
        ExtBuilder::default()
            .with_balances(vec![(account_id, 30), (account_id_2, 10)])
            .with_candidates(vec![(account_id, 30)])
            .build()
            .execute_with(|| {
                assert_eq!(
                    ParachainStaking::get_nominator_stakable_free_balance(&account_id_2),
                    10
                );
                assert_ok!(ParachainStaking::nominate(
                    Origin::signed(account_id_2),
                    account_id,
                    10,
                    0,
                    0
                ));
                assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&account_id_2), 0);
            });
    }

    #[test]
    fn nominate_updates_nominator_state() {
        let account_id = to_acc_id(1u64);
        let account_id_2 = to_acc_id(2u64);
        ExtBuilder::default()
            .with_balances(vec![(account_id, 30), (account_id_2, 10)])
            .with_candidates(vec![(account_id, 30)])
            .build()
            .execute_with(|| {
                assert!(ParachainStaking::nominator_state(account_id_2).is_none());
                assert_ok!(ParachainStaking::nominate(
                    Origin::signed(account_id_2),
                    account_id,
                    10,
                    0,
                    0
                ));
                let nominator_state = ParachainStaking::nominator_state(account_id_2)
                    .expect("just nominated => exists");
                assert_eq!(nominator_state.total(), 10);
                assert_eq!(nominator_state.nominations.0[0].owner, account_id);
                assert_eq!(nominator_state.nominations.0[0].amount, 10);
            });
    }

    #[test]
    fn nominate_updates_collator_state() {
        let account_id = to_acc_id(1u64);
        let account_id_2 = to_acc_id(2u64);
        ExtBuilder::default()
            .with_balances(vec![(account_id, 30), (account_id_2, 10)])
            .with_candidates(vec![(account_id, 30)])
            .build()
            .execute_with(|| {
                let candidate_state =
                    ParachainStaking::candidate_info(account_id).expect("registered in genesis");
                assert_eq!(candidate_state.total_counted, 30);
                let top_nominations =
                    ParachainStaking::top_nominations(account_id).expect("registered in genesis");
                assert!(top_nominations.nominations.is_empty());
                assert!(top_nominations.total.is_zero());
                assert_ok!(ParachainStaking::nominate(
                    Origin::signed(account_id_2),
                    account_id,
                    10,
                    0,
                    0
                ));
                let candidate_state =
                    ParachainStaking::candidate_info(account_id).expect("just nominated => exists");
                assert_eq!(candidate_state.total_counted, 40);
                let top_nominations = ParachainStaking::top_nominations(account_id)
                    .expect("just nominated => exists");
                assert_eq!(top_nominations.nominations[0].owner, account_id_2);
                assert_eq!(top_nominations.nominations[0].amount, 10);
                assert_eq!(top_nominations.total, 10);
            });
    }

    #[test]
    fn can_nominate_immediately_after_other_join_candidates() {
        let account_id = to_acc_id(1u64);
        let account_id_2 = to_acc_id(2u64);
        ExtBuilder::default()
            .with_balances(vec![(account_id, 20), (account_id_2, 20)])
            .build()
            .execute_with(|| {
                assert_ok!(ParachainStaking::join_candidates(Origin::signed(account_id), 20, 0));
                assert_ok!(ParachainStaking::nominate(
                    Origin::signed(account_id_2),
                    account_id,
                    20,
                    0,
                    0
                ));
            });
    }

    #[test]
    fn can_nominate_if_revoking() {
        let account_id = to_acc_id(1u64);
        let account_id_2 = to_acc_id(2u64);
        let account_id_3 = to_acc_id(3u64);
        let account_id_4 = to_acc_id(4u64);
        ExtBuilder::default()
            .with_balances(vec![
                (account_id, 20),
                (account_id_2, 30),
                (account_id_3, 20),
                (account_id_4, 20),
            ])
            .with_candidates(vec![(account_id, 20), (account_id_3, 20), (account_id_4, 20)])
            .with_nominations(vec![
                (account_id_2, account_id, 10),
                (account_id_2, account_id_3, 10),
            ])
            .build()
            .execute_with(|| {
                assert_ok!(ParachainStaking::schedule_revoke_nomination(
                    Origin::signed(account_id_2),
                    account_id
                ));
                assert_ok!(ParachainStaking::nominate(
                    Origin::signed(account_id_2),
                    account_id_4,
                    10,
                    0,
                    2
                ));
            });
    }

    #[test]
    fn cannot_nominate_if_full_and_new_nomination_less_than_or_equal_lowest_bottom() {
        let account_id = to_acc_id(1u64);
        ExtBuilder::default()
            .with_balances(vec![
                (account_id, 20),
                (to_acc_id(2), 10),
                (to_acc_id(3), 10),
                (to_acc_id(4), 10),
                (to_acc_id(5), 10),
                (to_acc_id(6), 10),
                (to_acc_id(7), 10),
                (to_acc_id(8), 10),
                (to_acc_id(9), 10),
                (to_acc_id(10), 10),
                (to_acc_id(11), 11),
            ])
            .with_candidates(vec![(account_id, 20)])
            .with_nominations(vec![
                (to_acc_id(2), account_id, 10),
                (to_acc_id(3), account_id, 10),
                (to_acc_id(4), account_id, 10),
                (to_acc_id(5), account_id, 10),
                (to_acc_id(6), account_id, 10),
                (to_acc_id(8), account_id, 10),
                (to_acc_id(9), account_id, 10),
                (to_acc_id(10), account_id, 10),
            ])
            .build()
            .execute_with(|| {
                assert_noop!(
                    ParachainStaking::nominate(Origin::signed(to_acc_id(11)), account_id, 10, 8, 0),
                    Error::<Test>::CannotNominateLessThanOrEqualToLowestBottomWhenFull
                );
            });
    }

    #[test]
    fn can_nominate_if_full_and_new_nomination_greater_than_lowest_bottom() {
        let account_id = to_acc_id(1u64);
        ExtBuilder::default()
            .with_balances(vec![
                (account_id, 20),
                (to_acc_id(2), 10),
                (to_acc_id(3), 10),
                (to_acc_id(4), 10),
                (to_acc_id(5), 10),
                (to_acc_id(6), 10),
                (to_acc_id(7), 10),
                (to_acc_id(8), 10),
                (to_acc_id(9), 10),
                (to_acc_id(10), 10),
                (to_acc_id(11), 11),
            ])
            .with_candidates(vec![(account_id, 20)])
            .with_nominations(vec![
                (to_acc_id(2), account_id, 10),
                (to_acc_id(3), account_id, 10),
                (to_acc_id(4), account_id, 10),
                (to_acc_id(5), account_id, 10),
                (to_acc_id(6), account_id, 10),
                (to_acc_id(8), account_id, 10),
                (to_acc_id(9), account_id, 10),
                (to_acc_id(10), account_id, 10),
            ])
            .build()
            .execute_with(|| {
                assert_ok!(ParachainStaking::nominate(
                    Origin::signed(to_acc_id(11)),
                    account_id,
                    11,
                    8,
                    0
                ));
                assert_event_emitted!(Event::NominationKicked {
                    nominator: to_acc_id(10),
                    candidate: account_id,
                    unstaked_amount: 10
                });
                assert_event_emitted!(Event::NominatorLeft {
                    nominator: to_acc_id(10),
                    unstaked_amount: 10
                });
            });
    }

    #[test]
    fn can_still_nominate_if_leaving() {
        let account_id = to_acc_id(1u64);
        let account_id_2 = to_acc_id(2u64);
        let account_id_3 = to_acc_id(3u64);
        ExtBuilder::default()
            .with_balances(vec![(account_id, 20), (account_id_2, 20), (account_id_3, 20)])
            .with_candidates(vec![(account_id, 20), (account_id_3, 20)])
            .with_nominations(vec![(account_id_2, account_id, 10)])
            .build()
            .execute_with(|| {
                assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(
                    account_id_2
                )));
                assert_ok!(ParachainStaking::nominate(
                    Origin::signed(account_id_2),
                    account_id_3,
                    10,
                    0,
                    1
                ),);
            });
    }

    #[test]
    fn cannot_nominate_if_candidate() {
        let account_id = to_acc_id(1u64);
        let account_id_2 = to_acc_id(2u64);
        ExtBuilder::default()
            .with_balances(vec![(account_id, 20), (account_id_2, 30)])
            .with_candidates(vec![(account_id, 20), (account_id_2, 20)])
            .build()
            .execute_with(|| {
                assert_noop!(
                    ParachainStaking::nominate(Origin::signed(account_id_2), account_id, 10, 0, 0),
                    Error::<Test>::CandidateExists
                );
            });
    }

    #[test]
    fn cannot_nominate_if_already_nominated() {
        let account_id = to_acc_id(1u64);
        let account_id_2 = to_acc_id(2u64);
        ExtBuilder::default()
            .with_balances(vec![(account_id, 20), (account_id_2, 30)])
            .with_candidates(vec![(account_id, 20)])
            .with_nominations(vec![(account_id_2, account_id, 20)])
            .build()
            .execute_with(|| {
                assert_noop!(
                    ParachainStaking::nominate(Origin::signed(account_id_2), account_id, 10, 1, 1),
                    Error::<Test>::AlreadyNominatedCandidate
                );
            });
    }

    #[test]
    fn cannot_nominate_more_than_max_nominations() {
        let account_id = to_acc_id(1u64);
        ExtBuilder::default()
            .with_balances(vec![
                (account_id, 20),
                (to_acc_id(2), 50),
                (to_acc_id(3), 20),
                (to_acc_id(4), 20),
                (to_acc_id(5), 20),
                (to_acc_id(6), 20),
            ])
            .with_candidates(vec![
                (account_id, 20),
                (to_acc_id(3), 20),
                (to_acc_id(4), 20),
                (to_acc_id(5), 20),
                (to_acc_id(6), 20),
            ])
            .with_nominations(vec![
                (to_acc_id(2), account_id, 10),
                (to_acc_id(2), to_acc_id(3), 10),
                (to_acc_id(2), to_acc_id(4), 10),
                (to_acc_id(2), to_acc_id(5), 10),
            ])
            .build()
            .execute_with(|| {
                assert_noop!(
                    ParachainStaking::nominate(
                        Origin::signed(to_acc_id(2)),
                        to_acc_id(6),
                        10,
                        0,
                        4
                    ),
                    Error::<Test>::ExceedMaxNominationsPerNominator,
                );
            });
    }

    #[test]
    fn sufficient_nominate_weight_hint_succeeds() {
        let account_id = to_acc_id(1u64);
        ExtBuilder::default()
            .with_balances(vec![
                (account_id, 20),
                (to_acc_id(2), 20),
                (to_acc_id(3), 20),
                (to_acc_id(4), 20),
                (to_acc_id(5), 20),
                (to_acc_id(6), 20),
                (to_acc_id(7), 20),
                (to_acc_id(8), 20),
                (to_acc_id(9), 20),
                (to_acc_id(10), 20),
            ])
            .with_candidates(vec![(account_id, 20), (to_acc_id(2), 20)])
            .with_nominations(vec![
                (to_acc_id(3), account_id, 10),
                (to_acc_id(4), account_id, 10),
                (to_acc_id(5), account_id, 10),
                (to_acc_id(6), account_id, 10),
            ])
            .build()
            .execute_with(|| {
                let mut count = 4u32;
                for i in 7..11 {
                    assert_ok!(ParachainStaking::nominate(
                        Origin::signed(to_acc_id(i)),
                        account_id,
                        10,
                        count,
                        0u32
                    ));
                    count += 1u32;
                }
                let mut count = 0u32;
                for i in 3..11 {
                    assert_ok!(ParachainStaking::nominate(
                        Origin::signed(to_acc_id(i)),
                        to_acc_id(2),
                        10,
                        count,
                        1u32
                    ));
                    count += 1u32;
                }
            });
    }

    #[test]
    fn insufficient_nominate_weight_hint_fails() {
        let account_id = to_acc_id(1u64);
        ExtBuilder::default()
            .with_balances(vec![
                (account_id, 20),
                (to_acc_id(2), 20),
                (to_acc_id(3), 20),
                (to_acc_id(4), 20),
                (to_acc_id(5), 20),
                (to_acc_id(6), 20),
                (to_acc_id(7), 20),
                (to_acc_id(8), 20),
                (to_acc_id(9), 20),
                (to_acc_id(10), 20),
            ])
            .with_candidates(vec![(account_id, 20), (to_acc_id(2), 20)])
            .with_nominations(vec![
                (to_acc_id(3), account_id, 10),
                (to_acc_id(4), account_id, 10),
                (to_acc_id(5), account_id, 10),
                (to_acc_id(6), account_id, 10),
            ])
            .build()
            .execute_with(|| {
                let mut count = 3u32;
                for i in 7..11 {
                    assert_noop!(
                        ParachainStaking::nominate(
                            Origin::signed(to_acc_id(i)),
                            account_id,
                            10,
                            count,
                            0u32
                        ),
                        Error::<Test>::TooLowCandidateNominationCountToNominate
                    );
                }
                // to set up for next error test
                count = 4u32;
                for i in 7..11 {
                    assert_ok!(ParachainStaking::nominate(
                        Origin::signed(to_acc_id(i)),
                        account_id,
                        10,
                        count,
                        0u32
                    ));
                    count += 1u32;
                }
                count = 0u32;
                for i in 3..11 {
                    assert_noop!(
                        ParachainStaking::nominate(
                            Origin::signed(to_acc_id(i)),
                            to_acc_id(2),
                            10,
                            count,
                            0u32
                        ),
                        Error::<Test>::TooLowNominationCountToNominate
                    );
                    count += 1u32;
                }
            });
    }
}
