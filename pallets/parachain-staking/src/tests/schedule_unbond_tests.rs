//Copyright 2022 Aventus Network Services.

#![cfg(test)]

use crate::{
    assert_event_emitted, encode_signed_execute_nomination_request_params,
    encode_signed_schedule_candidate_unbond_params, encode_signed_schedule_nominator_unbond_params,
    mock::{
        build_proof, roll_to_era_begin, sign, AccountId, AvnProxy, Call as MockCall, ExtBuilder,
        MinNominationPerCollator, Origin, ParachainStaking, Signature, Staker, System, Test,
        TestAccount,
    },
    Config, Error, Event, Proof,
};
use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use frame_system::{self as system, RawOrigin};
use std::cell::RefCell;

thread_local! {
    pub static AMOUNT_TO_UNBOND: RefCell<u128> = RefCell::new(0u128);
}

fn to_acc_id(id: u64) -> AccountId {
    return TestAccount::new(id).account_id()
}

fn get_accounts(
    num: u32,
    initial_balance: u128,
    additional: Option<Vec<(AccountId, u128)>>,
) -> Vec<(AccountId, u128)> {
    let mut balances: Vec<(AccountId, u128)> = vec![];

    for i in 1..=num {
        balances.push((to_acc_id(i as u64), initial_balance));
    }

    if additional.is_some() {
        balances = [balances, additional.unwrap()].concat()
    }

    return balances
}

fn get_nominations(
    num: u32,
    nominator: AccountId,
    nominations: &Vec<u128>,
) -> Vec<(AccountId, AccountId, u128)> {
    let mut accounts: Vec<(AccountId, AccountId)> = vec![];

    for i in 1..=num {
        accounts.push((nominator, to_acc_id(i as u64)));
    }

    return accounts
        .into_iter()
        .zip(nominations)
        .map(|v| (v.0 .0, v.0 .1, *v.1))
        .collect::<Vec<_>>()
}

fn get_max_to_unbond(nominations: &Vec<u128>, num_collators: u32) -> u128 {
    let total_nominations = nominations.iter().sum::<u128>();
    return total_nominations - (MinNominationPerCollator::get() * num_collators as u128)
}

mod proxy_signed_schedule_unbond {
    use super::*;

    pub fn create_call_for_signed_schedule_nominator_unbond(
        staker: &Staker,
        sender_nonce: u64,
        reduction_amount: u128,
    ) -> Box<<Test as Config>::Call> {
        let proof = create_proof_for_signed_schedule_nominator_unbond(
            sender_nonce,
            staker,
            &reduction_amount,
        );

        return Box::new(MockCall::ParachainStaking(
            super::super::Call::<Test>::signed_schedule_nominator_unbond {
                proof,
                less: reduction_amount,
            },
        ))
    }

    fn create_proof_for_signed_schedule_nominator_unbond(
        sender_nonce: u64,
        staker: &Staker,
        reduction_amount: &u128,
    ) -> Proof<Signature, AccountId> {
        let data_to_sign = encode_signed_schedule_nominator_unbond_params::<Test>(
            staker.relayer.clone(),
            reduction_amount,
            sender_nonce,
        );

        let signature = sign(&staker.key_pair, &data_to_sign);
        return build_proof(&staker.account_id, &staker.relayer, signature)
    }

    fn unbond_event_emitted(nominator: AccountId) -> bool {
        System::events()
            .into_iter()
            .map(|r| r.event)
            .filter_map(|e| {
                if let crate::mock::Event::ParachainStaking(inner) = e {
                    Some(inner)
                } else {
                    None
                }
            })
            .filter_map(|inner| {
                if let Event::NominationDecreaseScheduled { nominator, .. } = inner {
                    Some(nominator)
                } else {
                    None
                }
            })
            .any(|n| n == nominator)
    }

    #[test]
    fn succeeds_with_good_values() {
        ExtBuilder::default().build().execute_with(|| {
            let num_collators = 10;

            let mut nominations: Vec<u128> = vec![7, 7, 7, 7, 7, 7, 7, 7, 7, 7];
            for i in 1..=get_max_to_unbond(&nominations, num_collators) {
                AMOUNT_TO_UNBOND.with(|pk| *pk.borrow_mut() = i);
                unbond(num_collators.clone(), &nominations);
            }

            nominations = vec![6, 6, 6, 6, 6, 6, 6, 6, 6, 11];
            for i in 1..=get_max_to_unbond(&nominations, num_collators) {
                AMOUNT_TO_UNBOND.with(|pk| *pk.borrow_mut() = i);
                unbond(num_collators.clone(), &nominations);
            }

            nominations = vec![15, 11, 11, 11, 13, 19, 19, 10, 10, 14];
            for i in 1..=get_max_to_unbond(&nominations, num_collators) {
                AMOUNT_TO_UNBOND.with(|pk| *pk.borrow_mut() = i);
                unbond(num_collators.clone(), &nominations);
            }

            nominations = vec![102, 4, 13, 25, 21, 3, 49, 11, 39, 87];
            for i in 1..=get_max_to_unbond(&nominations, num_collators) {
                AMOUNT_TO_UNBOND.with(|pk| *pk.borrow_mut() = i);
                unbond(num_collators.clone(), &nominations);
            }
        });
    }

    fn unbond(num_collators: u32, nominations: &Vec<u128>) {
        let initial_collator_stake = 10;
        let initial_balance = 1000000000000000000000;
        let staker: Staker = Default::default();
        let staker_balance =
            vec![(staker.account_id, initial_balance), (staker.relayer, initial_balance)];
        ExtBuilder::default()
            .with_balances(get_accounts(num_collators, initial_balance, Some(staker_balance)))
            .with_candidates(get_accounts(num_collators, initial_collator_stake, None))
            .with_nominations(get_nominations(num_collators, staker.account_id, nominations))
            .with_staking_config(
                initial_collator_stake,
                MinNominationPerCollator::get() * num_collators as u128,
            )
            .build()
            .execute_with(|| {
                let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                let unbond_call = create_call_for_signed_schedule_nominator_unbond(
                    &staker,
                    nonce,
                    AMOUNT_TO_UNBOND.with(|v| *v.borrow()),
                );
                assert_ok!(AvnProxy::proxy(Origin::signed(staker.relayer), unbond_call, None));

                assert!(unbond_event_emitted(Staker::default().account_id));

                // Nonce has increased
                assert_eq!(ParachainStaking::proxy_nonce(staker.account_id), nonce + 1);
            });
    }

    mod fails_when {
        use super::*;

        #[test]
        fn extrinsic_is_unsigned() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let staker: Staker = Default::default();
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_1, 10000),
                    (collator_2, 10000),
                    (staker.account_id, 10000),
                    (staker.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1, 10), (collator_2, 10)])
                .with_nominations(vec![
                    (staker.account_id, collator_1, 10),
                    (staker.account_id, collator_2, 10),
                ])
                .build()
                .execute_with(|| {
                    let amount_to_withdraw = 10;
                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let proof = create_proof_for_signed_schedule_nominator_unbond(
                        nonce,
                        &staker,
                        &amount_to_withdraw,
                    );

                    assert_noop!(
                        ParachainStaking::signed_schedule_nominator_unbond(
                            RawOrigin::None.into(),
                            proof.clone(),
                            amount_to_withdraw
                        ),
                        BadOrigin
                    );

                    // Show that we can send a successful transaction if it's signed.
                    assert_ok!(ParachainStaking::signed_schedule_nominator_unbond(
                        Origin::signed(staker.account_id),
                        proof,
                        amount_to_withdraw
                    ));
                });
        }

        #[test]
        fn proxy_proof_is_not_valid() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let staker: Staker = Default::default();
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_1, 10000),
                    (collator_2, 10000),
                    (staker.account_id, 10000),
                    (staker.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1, 10), (collator_2, 10)])
                .with_nominations(vec![
                    (staker.account_id, collator_1, 10),
                    (staker.account_id, collator_2, 10),
                ])
                .build()
                .execute_with(|| {
                    let amount_to_withdraw = 10;
                    let bad_nonce = ParachainStaking::proxy_nonce(staker.account_id) + 1;
                    let unbond_call = create_call_for_signed_schedule_nominator_unbond(
                        &staker,
                        bad_nonce,
                        amount_to_withdraw,
                    );

                    assert_noop!(
                        AvnProxy::proxy(Origin::signed(staker.relayer), unbond_call, None),
                        Error::<Test>::UnauthorizedSignedUnbondTransaction
                    );
                });
        }

        #[test]
        fn staker_does_not_have_enough_to_withdraw() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let staker: Staker = Default::default();
            let staker_balance = 100;
            let initial_stake = 10;
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_1, 10000),
                    (collator_2, 10000),
                    (staker.account_id, staker_balance),
                    (staker.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1, initial_stake), (collator_2, initial_stake)])
                .with_nominations(vec![
                    (staker.account_id, collator_1, initial_stake),
                    (staker.account_id, collator_2, initial_stake),
                ])
                .build()
                .execute_with(|| {
                    let total_stake = staker_balance * 2;
                    // amount falls below min total stake
                    let bad_amount_to_unbond =
                        total_stake - ParachainStaking::min_total_nominator_stake() - 1;

                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let unbond_call = create_call_for_signed_schedule_nominator_unbond(
                        &staker,
                        nonce,
                        bad_amount_to_unbond,
                    );

                    assert_noop!(
                        AvnProxy::proxy(Origin::signed(staker.relayer), unbond_call, None),
                        Error::<Test>::NominatorBondBelowMin
                    );
                });
        }

        #[test]
        fn withdrawal_reduces_per_collator_bond_below_min_allowed() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let staker: Staker = Default::default();
            let staker_balance = 10000;
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_1, 10000),
                    (collator_2, 10000),
                    (staker.account_id, staker_balance),
                    (staker.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1, 10), (collator_2, 10)])
                .with_nominations(vec![
                    (staker.account_id, collator_1, 10),
                    (staker.account_id, collator_2, 10),
                ])
                .with_staking_config(10, 4u128)
                .build()
                .execute_with(|| {
                    let total_stake = 20;
                    let bad_amount_to_unbond =
                        (total_stake - (2 * MinNominationPerCollator::get())) + 1;

                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let unbond_call = create_call_for_signed_schedule_nominator_unbond(
                        &staker,
                        nonce,
                        bad_amount_to_unbond,
                    );

                    assert_noop!(
                        AvnProxy::proxy(Origin::signed(staker.relayer), unbond_call, None),
                        Error::<Test>::NominationBelowMin
                    );
                });
        }
    }
}

mod proxy_signed_schedule_collator_unbond {
    use super::*;

    fn create_call_for_signed_schedule_candidate_unbond(
        staker: &Staker,
        sender_nonce: u64,
        reduction_amount: u128,
    ) -> Box<<Test as Config>::Call> {
        let proof = create_proof_for_signed_schedule_candidate_unbond(
            sender_nonce,
            staker,
            &reduction_amount,
        );

        return Box::new(MockCall::ParachainStaking(
            super::super::Call::<Test>::signed_schedule_candidate_unbond {
                proof,
                less: reduction_amount,
            },
        ))
    }

    fn create_proof_for_signed_schedule_candidate_unbond(
        sender_nonce: u64,
        staker: &Staker,
        reduction_amount: &u128,
    ) -> Proof<Signature, AccountId> {
        let data_to_sign = encode_signed_schedule_candidate_unbond_params::<Test>(
            staker.relayer.clone(),
            reduction_amount,
            sender_nonce,
        );

        let signature = sign(&staker.key_pair, &data_to_sign);
        return build_proof(&staker.account_id, &staker.relayer, signature)
    }

    #[test]
    fn succeeds_with_good_values() {
        let collator_1: Staker = Default::default();
        let collator_2 = to_acc_id(2u64);
        let initial_stake = 100;
        ExtBuilder::default()
            .with_balances(vec![
                (collator_1.account_id, 10000),
                (collator_2, 10000),
                (collator_1.relayer, 10000),
            ])
            .with_candidates(vec![
                (collator_1.account_id, initial_stake),
                (collator_2, initial_stake),
            ])
            .build()
            .execute_with(|| {
                let min_collator_stake = ParachainStaking::min_collator_stake();

                let amount_to_decrease = initial_stake - min_collator_stake;
                let nonce = ParachainStaking::proxy_nonce(collator_1.account_id);
                let candidate_unbond_call = create_call_for_signed_schedule_candidate_unbond(
                    &collator_1,
                    nonce,
                    amount_to_decrease,
                );

                assert_ok!(AvnProxy::proxy(
                    Origin::signed(collator_1.relayer),
                    candidate_unbond_call,
                    None
                ));

                assert_event_emitted!(Event::CandidateBondLessRequested {
                    candidate: collator_1.account_id,
                    amount_to_decrease,
                    execute_era: ParachainStaking::delay() + 1,
                });

                // Nonce has increased
                assert_eq!(ParachainStaking::proxy_nonce(collator_1.account_id), nonce + 1);
            });
    }

    mod fails_when {
        use super::*;

        #[test]
        fn extrinsic_is_unsigned() {
            let collator_1: Staker = Default::default();
            let collator_2 = to_acc_id(2u64);
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_2, 10000),
                    (collator_1.account_id, 10000),
                    (collator_1.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1.account_id, 100), (collator_2, 100)])
                .build()
                .execute_with(|| {
                    let amount_to_withdraw = 10;
                    let nonce = ParachainStaking::proxy_nonce(collator_1.account_id);
                    let proof = create_proof_for_signed_schedule_candidate_unbond(
                        nonce,
                        &collator_1,
                        &amount_to_withdraw,
                    );

                    assert_noop!(
                        ParachainStaking::signed_schedule_candidate_unbond(
                            RawOrigin::None.into(),
                            proof.clone(),
                            amount_to_withdraw
                        ),
                        BadOrigin
                    );

                    // Show that we can send a successful transaction if its signed.
                    assert_ok!(ParachainStaking::signed_schedule_candidate_unbond(
                        Origin::signed(collator_1.account_id),
                        proof,
                        amount_to_withdraw
                    ));
                });
        }

        #[test]
        fn proxy_proof_is_not_valid() {
            let collator_1: Staker = Default::default();
            let collator_2 = to_acc_id(2u64);
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_2, 10000),
                    (collator_1.account_id, 10000),
                    (collator_1.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1.account_id, 100), (collator_2, 100)])
                .build()
                .execute_with(|| {
                    let amount_to_withdraw = 10;
                    let bad_nonce = ParachainStaking::proxy_nonce(collator_1.account_id) + 1;
                    let unbond_call = create_call_for_signed_schedule_candidate_unbond(
                        &collator_1,
                        bad_nonce,
                        amount_to_withdraw,
                    );

                    assert_noop!(
                        AvnProxy::proxy(Origin::signed(collator_1.relayer), unbond_call, None),
                        Error::<Test>::UnauthorizedSignedCandidateUnbondTransaction
                    );
                });
        }

        #[test]
        fn withdrawal_reduces_candidate_bond_below_min_allowed() {
            let collator_1: Staker = Default::default();
            let collator_2 = to_acc_id(2u64);
            let collator_stake = 100;
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_2, 10000),
                    (collator_1.account_id, 10000),
                    (collator_1.relayer, 10000),
                ])
                .with_candidates(vec![
                    (collator_1.account_id, collator_stake),
                    (collator_2, collator_stake),
                ])
                .build()
                .execute_with(|| {
                    let amount_to_withdraw =
                        (collator_stake - ParachainStaking::min_collator_stake()) + 1;
                    let nonce = ParachainStaking::proxy_nonce(collator_1.account_id);
                    let unbond_call = create_call_for_signed_schedule_candidate_unbond(
                        &collator_1,
                        nonce,
                        amount_to_withdraw,
                    );

                    assert_noop!(
                        AvnProxy::proxy(Origin::signed(collator_1.relayer), unbond_call, None),
                        Error::<Test>::CandidateBondBelowMin
                    );
                });
        }
    }
}

mod signed_execute_nomination_request {
    use super::*;
    use crate::schedule_unbond_tests::proxy_signed_schedule_unbond::create_call_for_signed_schedule_nominator_unbond;

    fn schedule_unbond(staker: &Staker, amount: &u128) {
        let nonce = ParachainStaking::proxy_nonce(staker.account_id);
        let unbond_call = create_call_for_signed_schedule_nominator_unbond(staker, nonce, *amount);

        assert_ok!(AvnProxy::proxy(Origin::signed(staker.relayer), unbond_call, None));
    }

    fn create_call_for_signed_execute_nomination_request(
        staker: &Staker,
        sender_nonce: u64,
        nominator: AccountId,
    ) -> Box<<Test as Config>::Call> {
        let proof =
            create_proof_for_signed_execute_nomination_request(sender_nonce, staker, &nominator);

        return Box::new(MockCall::ParachainStaking(
            super::super::Call::<Test>::signed_execute_nomination_request { proof, nominator },
        ))
    }

    fn create_proof_for_signed_execute_nomination_request(
        sender_nonce: u64,
        staker: &Staker,
        nominator: &AccountId,
    ) -> Proof<Signature, AccountId> {
        let data_to_sign = encode_signed_execute_nomination_request_params::<Test>(
            staker.relayer.clone(),
            nominator,
            sender_nonce,
        );

        let signature = sign(&staker.key_pair, &data_to_sign);
        return build_proof(&staker.account_id, &staker.relayer, signature)
    }

    #[test]
    fn suceeds_with_good_values() {
        let collator_1 = to_acc_id(1u64);
        let collator_2 = to_acc_id(2u64);
        let staker: Staker = Default::default();
        let initial_stake = 100;
        let initial_balance = 10000;
        ExtBuilder::default()
            .with_balances(vec![
                (collator_1, initial_balance),
                (collator_2, initial_balance),
                (staker.account_id, initial_balance),
                (staker.relayer, initial_balance),
            ])
            .with_candidates(vec![(collator_1, initial_stake), (collator_2, initial_stake)])
            .with_nominations(vec![
                (staker.account_id, collator_1, initial_stake),
                (staker.account_id, collator_2, initial_stake),
            ])
            .build()
            .execute_with(|| {
                let amount_to_unbond = 100;
                let initial_free_balance =
                    ParachainStaking::get_nominator_stakable_free_balance(&staker.account_id);
                let initial_total_stake = ParachainStaking::total();
                let initial_state_total =
                    ParachainStaking::nominator_state(staker.account_id).unwrap().total();
                let initial_collator1_state =
                    &ParachainStaking::top_nominations(collator_1).unwrap().nominations[0];
                let initial_collator2_state =
                    &ParachainStaking::top_nominations(collator_2).unwrap().nominations[0];

                schedule_unbond(&staker, &amount_to_unbond);
                roll_to_era_begin((ParachainStaking::delay() + 1u32) as u64);

                let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                let execute_unbond_call = create_call_for_signed_execute_nomination_request(
                    &staker,
                    nonce,
                    staker.account_id,
                );

                assert_ok!(AvnProxy::proxy(
                    Origin::signed(staker.relayer),
                    execute_unbond_call,
                    None
                ));

                // Nonce has increased
                assert_eq!(ParachainStaking::proxy_nonce(staker.account_id), nonce + 1);
                assert_eq!(
                    ParachainStaking::get_nominator_stakable_free_balance(&staker.account_id),
                    initial_free_balance + amount_to_unbond
                );
                assert_eq!(ParachainStaking::total(), initial_total_stake - amount_to_unbond);
                assert_eq!(
                    ParachainStaking::nominator_state(staker.account_id).unwrap().total(),
                    initial_state_total - amount_to_unbond
                );
                assert_eq!(
                    ParachainStaking::top_nominations(collator_1).unwrap().nominations[0].amount,
                    initial_collator1_state.amount - 50
                );
                assert_eq!(
                    ParachainStaking::top_nominations(collator_2).unwrap().nominations[0].amount,
                    initial_collator2_state.amount - 50
                );
            });
    }

    mod fails_when {
        use super::*;

        #[test]
        fn extrinsic_is_unsigned() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let staker: Staker = Default::default();
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_1, 10000),
                    (collator_2, 10000),
                    (staker.account_id, 10000),
                    (staker.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1, 100), (collator_2, 100)])
                .with_nominations(vec![
                    (staker.account_id, collator_1, 100),
                    (staker.account_id, collator_2, 100),
                ])
                .build()
                .execute_with(|| {
                    let amount_to_unbond = 100;
                    schedule_unbond(&staker, &amount_to_unbond);
                    roll_to_era_begin((ParachainStaking::delay() + 1u32) as u64);

                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let proof = create_proof_for_signed_execute_nomination_request(
                        nonce,
                        &staker,
                        &staker.account_id,
                    );

                    assert_noop!(
                        ParachainStaking::signed_execute_nomination_request(
                            RawOrigin::None.into(),
                            proof.clone(),
                            staker.account_id,
                        ),
                        BadOrigin
                    );

                    // Show that we can send a successful transaction if its signed.
                    assert_ok!(ParachainStaking::signed_execute_nomination_request(
                        Origin::signed(staker.account_id),
                        proof.clone(),
                        staker.account_id,
                    ));
                });
        }

        #[test]
        fn proxy_proof_is_not_valid_nonce() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let staker: Staker = Default::default();
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_1, 10000),
                    (collator_2, 10000),
                    (staker.account_id, 10000),
                    (staker.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1, 100), (collator_2, 100)])
                .with_nominations(vec![
                    (staker.account_id, collator_1, 100),
                    (staker.account_id, collator_2, 100),
                ])
                .build()
                .execute_with(|| {
                    let amount_to_unbond = 100;
                    schedule_unbond(&staker, &amount_to_unbond);
                    roll_to_era_begin((ParachainStaking::delay() + 1u32) as u64);

                    let bad_nonce = ParachainStaking::proxy_nonce(staker.account_id) + 1;
                    let execute_unbond_call = create_call_for_signed_execute_nomination_request(
                        &staker,
                        bad_nonce,
                        staker.account_id,
                    );

                    assert_noop!(
                        AvnProxy::proxy(Origin::signed(staker.relayer), execute_unbond_call, None),
                        Error::<Test>::UnauthorizedSignedExecuteNominationRequestTransaction
                    );
                });
        }

        #[test]
        fn proxy_proof_is_not_valid_nominator() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let staker: Staker = Default::default();
            ExtBuilder::default()
                .with_balances(vec![
                    (collator_1, 10000),
                    (collator_2, 10000),
                    (staker.account_id, 10000),
                    (staker.relayer, 10000),
                ])
                .with_candidates(vec![(collator_1, 100), (collator_2, 100)])
                .with_nominations(vec![
                    (staker.account_id, collator_1, 100),
                    (staker.account_id, collator_2, 100),
                ])
                .build()
                .execute_with(|| {
                    let amount_to_unbond = 100;
                    schedule_unbond(&staker, &amount_to_unbond);
                    roll_to_era_begin((ParachainStaking::delay() + 1u32) as u64);

                    let bad_nominator = staker.relayer;
                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let proof = create_proof_for_signed_execute_nomination_request(
                        nonce,
                        &staker,
                        &bad_nominator,
                    );

                    assert_noop!(
                        ParachainStaking::signed_execute_nomination_request(
                            Origin::signed(staker.account_id),
                            proof.clone(),
                            staker.account_id,
                        ),
                        Error::<Test>::UnauthorizedSignedExecuteNominationRequestTransaction
                    );
                });
        }
    }
}
