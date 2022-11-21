//Copyright 2022 Aventus Network Services.bond_extra

#![cfg(test)]

use crate::{
    assert_event_emitted, assert_last_event, encode_signed_nominate_params,
    mock::{
        build_proof, sign, AccountId, AvnProxy, Call as MockCall, Event as MetaEvent, ExtBuilder,
        Origin, ParachainStaking, Signature, Staker, Test, TestAccount,
    },
    Config, Error, Event, NominatorAdded, Proof, StaticLookup,
};
use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use frame_system::{self as system, RawOrigin};
use sp_runtime::traits::Zero;

fn to_acc_id(id: u64) -> AccountId {
    return TestAccount::new(id).account_id()
}

// NOMINATOR BOND EXTRA

#[test]
fn nominator_bond_extra_reserves_balance() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![(account_id, 30), (account_id_2, 15)])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![(account_id_2, account_id, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&account_id_2), 5);
            assert_ok!(ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5));
            assert_eq!(ParachainStaking::get_nominator_stakable_free_balance(&account_id_2), 0);
        });
}

#[test]
fn nominator_bond_extra_increases_total_staked() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![(account_id, 30), (account_id_2, 15)])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![(account_id_2, account_id, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 40);
            assert_ok!(ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5));
            assert_eq!(ParachainStaking::total(), 45);
        });
}

#[test]
fn nominator_bond_extra_updates_nominator_state() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![(account_id, 30), (account_id_2, 15)])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![(account_id_2, account_id, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(
                ParachainStaking::nominator_state(account_id_2).expect("exists").total(),
                10
            );
            assert_ok!(ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5));
            assert_eq!(
                ParachainStaking::nominator_state(account_id_2).expect("exists").total(),
                15
            );
        });
}

#[test]
fn nominator_bond_extra_updates_candidate_state_top_nominations() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![(account_id, 30), (account_id_2, 15)])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![(account_id_2, account_id, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(
                ParachainStaking::top_nominations(account_id).unwrap().nominations[0].owner,
                account_id_2
            );
            assert_eq!(
                ParachainStaking::top_nominations(account_id).unwrap().nominations[0].amount,
                10
            );
            assert_eq!(ParachainStaking::top_nominations(account_id).unwrap().total, 10);
            assert_ok!(ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5));
            assert_eq!(
                ParachainStaking::top_nominations(account_id).unwrap().nominations[0].owner,
                account_id_2
            );
            assert_eq!(
                ParachainStaking::top_nominations(account_id).unwrap().nominations[0].amount,
                15
            );
            assert_eq!(ParachainStaking::top_nominations(account_id).unwrap().total, 15);
        });
}

#[test]
fn nominator_bond_extra_updates_candidate_state_bottom_nominations() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![
            (account_id, 30),
            (account_id_2, 20),
            (to_acc_id(3), 20),
            (to_acc_id(4), 20),
            (to_acc_id(5), 20),
            (to_acc_id(6), 20),
        ])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![
            (account_id_2, account_id, 10),
            (to_acc_id(3), account_id, 20),
            (to_acc_id(4), account_id, 20),
            (to_acc_id(5), account_id, 20),
            (to_acc_id(6), account_id, 20),
        ])
        .build()
        .execute_with(|| {
            assert_eq!(
                ParachainStaking::bottom_nominations(account_id).expect("exists").nominations[0]
                    .owner,
                account_id_2
            );
            assert_eq!(
                ParachainStaking::bottom_nominations(account_id).expect("exists").nominations[0]
                    .amount,
                10
            );
            assert_eq!(ParachainStaking::bottom_nominations(account_id).unwrap().total, 10);
            assert_ok!(ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5));
            assert_last_event!(MetaEvent::ParachainStaking(Event::NominationIncreased {
                nominator: account_id_2,
                candidate: account_id,
                amount: 5,
                in_top: false
            }));
            assert_eq!(
                ParachainStaking::bottom_nominations(account_id).expect("exists").nominations[0]
                    .owner,
                account_id_2
            );
            assert_eq!(
                ParachainStaking::bottom_nominations(account_id).expect("exists").nominations[0]
                    .amount,
                15
            );
            assert_eq!(ParachainStaking::bottom_nominations(account_id).unwrap().total, 15);
        });
}

#[test]
fn nominator_bond_extra_increases_total() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![(account_id, 30), (account_id_2, 15)])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![(account_id_2, account_id, 10)])
        .build()
        .execute_with(|| {
            assert_eq!(ParachainStaking::total(), 40);
            assert_ok!(ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5));
            assert_eq!(ParachainStaking::total(), 45);
        });
}

#[test]
fn can_nominator_bond_extra_for_leaving_candidate() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![(account_id, 30), (account_id_2, 15)])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![(account_id_2, account_id, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_leave_candidates(Origin::signed(account_id), 1));
            assert_ok!(ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5));
        });
}

#[test]
fn nominator_bond_extra_disallowed_when_revoke_scheduled() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![(account_id, 30), (account_id_2, 25)])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![(account_id_2, account_id, 10)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_revoke_nomination(
                Origin::signed(account_id_2),
                account_id
            ));
            assert_noop!(
                ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5),
                <Error<Test>>::PendingNominationRevoke
            );
        });
}

#[test]
fn nominator_bond_extra_allowed_when_bond_decrease_scheduled() {
    let account_id = to_acc_id(1u64);
    let account_id_2 = to_acc_id(2u64);
    ExtBuilder::default()
        .with_balances(vec![(account_id, 30), (account_id_2, 25)])
        .with_candidates(vec![(account_id, 30)])
        .with_nominations(vec![(account_id_2, account_id, 15)])
        .build()
        .execute_with(|| {
            assert_ok!(ParachainStaking::schedule_nominator_bond_less(
                Origin::signed(account_id_2),
                account_id,
                5,
            ));
            assert_ok!(ParachainStaking::bond_extra(Origin::signed(account_id_2), account_id, 5));
        });
}
