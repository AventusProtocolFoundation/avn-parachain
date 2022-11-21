//Copyright 2022 Aventus Network Services.

#![cfg(test)]

use crate::{
    assert_event_emitted, encode_signed_schedule_nominator_unbond_params,
    mock::{
        build_proof, sign, AccountId, AvnProxy, Call as MockCall, ExtBuilder,
        MinNominationPerCollator, Origin, ParachainStaking, Signature, Staker, System, Test,
        TestAccount,
    },
    Config, EraIndex, Error, Event, Proof,
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

    fn create_call_for_signed_schedule_nominator_unbond(
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

                    // Show that we can send a successful transaction if its signed.
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
