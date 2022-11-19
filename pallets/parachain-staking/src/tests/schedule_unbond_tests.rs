//Copyright 2022 Aventus Network Services.

#![cfg(test)]

use crate::{
    encode_signed_schedule_nominator_unbond_params,
    mock::{
        build_proof, sign, AccountId, AvnProxy, Call as MockCall, ExtBuilder,
        MinNominationPerCollator, Origin, ParachainStaking, Signature, Staker, Test, TestAccount,
    },
    Config, Proof,
};
use frame_support::assert_ok;
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

    #[test]
    fn unbond_test() {
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

                // assert_ok!(ParachainStaking::signed_schedule_nominator_unbond(
                //     Origin::signed(staker.account_id),
                //     AMOUNT_TO_UNBOND.with(|v| *v.borrow())
                // ));
            });
    }
}
