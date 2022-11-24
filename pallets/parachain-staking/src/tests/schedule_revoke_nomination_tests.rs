//Copyright 2022 Aventus Network Services.

#![cfg(test)]

use crate::{
    assert_event_emitted, encode_signed_execute_leave_nominators_params,
    encode_signed_schedule_leave_nominators_params,
    encode_signed_schedule_revoke_nomination_params,
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

mod proxy_signed_schedule_revoke_nomination {
    use super::*;

    fn create_call_for_signed_schedule_revoke_nomination(
        staker: &Staker,
        sender_nonce: u64,
        collator: &AccountId,
    ) -> Box<<Test as Config>::Call> {
        let proof =
            create_proof_for_signed_schedule_revoke_nomination(sender_nonce, staker, &collator);

        return Box::new(MockCall::ParachainStaking(
            super::super::Call::<Test>::signed_schedule_revoke_nomination {
                proof,
                collator: collator.clone(),
            },
        ))
    }

    fn create_call_for_signed_schedule_revoke_nomination_proof(
        proof: Proof<Signature, AccountId>,
        collator: &AccountId,
    ) -> Box<<Test as Config>::Call> {
        return Box::new(MockCall::ParachainStaking(
            super::super::Call::<Test>::signed_schedule_revoke_nomination {
                proof,
                collator: collator.clone(),
            },
        ))
    }

    fn create_proof_for_signed_schedule_revoke_nomination(
        sender_nonce: u64,
        staker: &Staker,
        collator: &AccountId,
    ) -> Proof<Signature, AccountId> {
        let data_to_sign = encode_signed_schedule_revoke_nomination_params::<Test>(
            staker.relayer.clone(),
            collator,
            sender_nonce,
        );

        let signature = sign(&staker.key_pair, &data_to_sign);
        return build_proof(&staker.account_id, &staker.relayer, signature)
    }

    #[test]
    fn succeeds_with_good_values() {
        let collator_1 = to_acc_id(1u64);
        let collator_2 = to_acc_id(2u64);
        let staker: Staker = Default::default();
        let initial_stake = 100;
        ExtBuilder::default()
            .with_balances(vec![
                (collator_1, 10000),
                (collator_2, 10000),
                (staker.account_id, 10000),
                (staker.relayer, 10000),
            ])
            .with_candidates(vec![(collator_1, initial_stake), (collator_2, initial_stake)])
            .with_nominations(vec![
                (staker.account_id, collator_1, 10),
                (staker.account_id, collator_2, 10),
            ])
            .build()
            .execute_with(|| {
                let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                let revoke_nomination_call =
                    create_call_for_signed_schedule_revoke_nomination(&staker, nonce, &collator_1);

                assert_ok!(AvnProxy::proxy(
                    Origin::signed(staker.relayer),
                    revoke_nomination_call,
                    None
                ));

                assert_event_emitted!(Event::NominationRevocationScheduled {
                    era: 1,
                    nominator: staker.account_id,
                    candidate: collator_1,
                    scheduled_exit: ParachainStaking::delay() + 1,
                });

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
                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let proof = create_proof_for_signed_schedule_revoke_nomination(
                        nonce,
                        &staker,
                        &collator_1,
                    );

                    assert_noop!(
                        ParachainStaking::signed_schedule_revoke_nomination(
                            RawOrigin::None.into(),
                            proof.clone(),
                            collator_1
                        ),
                        BadOrigin
                    );

                    // Show that we can send a successful transaction if its signed.
                    assert_ok!(ParachainStaking::signed_schedule_revoke_nomination(
                        Origin::signed(staker.account_id),
                        proof,
                        collator_1
                    ));
                });
        }

        #[test]
        fn proxy_proof_nonce_is_not_valid() {
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
                    let bad_nonce = ParachainStaking::proxy_nonce(staker.account_id) + 1;
                    let proof = create_proof_for_signed_schedule_revoke_nomination(
                        bad_nonce,
                        &staker,
                        &collator_1,
                    );

                    assert_noop!(
                        ParachainStaking::signed_schedule_revoke_nomination(
                            Origin::signed(staker.account_id),
                            proof.clone(),
                            collator_1
                        ),
                        Error::<Test>::UnauthorizedSignedRemoveBondTransaction
                    );
                });
        }

        #[test]
        fn proxy_proof_collator_is_not_valid() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let bad_collator = to_acc_id(10000000u64);

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
                    let bad_nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let proof = create_proof_for_signed_schedule_revoke_nomination(
                        bad_nonce,
                        &staker,
                        &bad_collator,
                    );
                    assert_noop!(
                        ParachainStaking::signed_schedule_revoke_nomination(
                            Origin::signed(staker.account_id),
                            proof.clone(),
                            bad_collator
                        ),
                        Error::<Test>::NominationDNE
                    );
                });
        }
    }
}

mod proxy_signed_schedule_leave_nominators {
    use super::*;

    pub fn create_call_for_signed_schedule_leave_nominators(
        staker: &Staker,
        sender_nonce: u64,
    ) -> Box<<Test as Config>::Call> {
        let proof = create_proof_for_signed_schedule_leave_nominators(sender_nonce, staker);

        return Box::new(MockCall::ParachainStaking(
            super::super::Call::<Test>::signed_schedule_leave_nominators { proof },
        ))
    }

    fn create_proof_for_signed_schedule_leave_nominators(
        sender_nonce: u64,
        staker: &Staker,
    ) -> Proof<Signature, AccountId> {
        let data_to_sign = encode_signed_schedule_leave_nominators_params::<Test>(
            staker.relayer.clone(),
            sender_nonce,
        );

        let signature = sign(&staker.key_pair, &data_to_sign);
        return build_proof(&staker.account_id, &staker.relayer, signature)
    }

    #[test]
    fn succeeds_with_good_values() {
        let collator_1 = to_acc_id(1u64);
        let collator_2 = to_acc_id(2u64);
        let staker: Staker = Default::default();
        let initial_stake = 100;
        ExtBuilder::default()
            .with_balances(vec![
                (collator_1, 10000),
                (collator_2, 10000),
                (staker.account_id, 10000),
                (staker.relayer, 10000),
            ])
            .with_candidates(vec![(collator_1, initial_stake), (collator_2, initial_stake)])
            .with_nominations(vec![
                (staker.account_id, collator_1, 10),
                (staker.account_id, collator_2, 10),
            ])
            .build()
            .execute_with(|| {
                let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                let leave_nominators_call =
                    create_call_for_signed_schedule_leave_nominators(&staker, nonce);

                assert_ok!(AvnProxy::proxy(
                    Origin::signed(staker.relayer),
                    leave_nominators_call,
                    None
                ));

                assert_event_emitted!(Event::NominatorExitScheduled {
                    era: 1,
                    nominator: staker.account_id,
                    scheduled_exit: ParachainStaking::delay() + 1,
                });

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
                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let proof = create_proof_for_signed_schedule_leave_nominators(nonce, &staker);

                    assert_noop!(
                        ParachainStaking::signed_schedule_leave_nominators(
                            RawOrigin::None.into(),
                            proof.clone(),
                        ),
                        BadOrigin
                    );

                    // Show that we can send a successful transaction if its signed.
                    assert_ok!(ParachainStaking::signed_schedule_leave_nominators(
                        Origin::signed(staker.account_id),
                        proof,
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
                    let bad_nonce = ParachainStaking::proxy_nonce(staker.account_id) + 1;
                    let leave_nominators_call =
                        create_call_for_signed_schedule_leave_nominators(&staker, bad_nonce);

                    assert_noop!(
                        AvnProxy::proxy(
                            Origin::signed(staker.relayer),
                            leave_nominators_call,
                            None
                        ),
                        Error::<Test>::UnauthorizedSignedScheduleLeaveNominatorsTransaction
                    );
                });
        }
    }
}

mod proxy_signed_execute_revoke_all_nomination {
    use super::*;

    use crate::schedule_revoke_nomination_tests::proxy_signed_schedule_leave_nominators::create_call_for_signed_schedule_leave_nominators;

    fn schedule_leave(staker: Staker) {
        let nonce = ParachainStaking::proxy_nonce(staker.account_id);
        let leave_nominators_call =
            create_call_for_signed_schedule_leave_nominators(&staker, nonce);

        assert_ok!(AvnProxy::proxy(Origin::signed(staker.relayer), leave_nominators_call, None));
    }

    fn create_call_for_signed_execute_leave_nominators(
        staker: &Staker,
        sender_nonce: u64,
        nominator: &AccountId,
    ) -> Box<<Test as Config>::Call> {
        let proof =
            create_proof_for_signed_execute_leave_nominators(sender_nonce, staker, nominator);

        return Box::new(MockCall::ParachainStaking(
            super::super::Call::<Test>::signed_execute_leave_nominators {
                proof,
                nominator: nominator.clone(),
            },
        ))
    }

    fn create_call_for_signed_execute_leave_nominators_from_proof(
        proof: Proof<Signature, AccountId>,
        nominator: &AccountId,
    ) -> Box<<Test as Config>::Call> {
        return Box::new(MockCall::ParachainStaking(
            super::super::Call::<Test>::signed_execute_leave_nominators {
                proof,
                nominator: nominator.clone(),
            },
        ))
    }

    fn create_proof_for_signed_execute_leave_nominators(
        sender_nonce: u64,
        staker: &Staker,
        nominator: &AccountId,
    ) -> Proof<Signature, AccountId> {
        let data_to_sign = encode_signed_execute_leave_nominators_params::<Test>(
            staker.relayer.clone(),
            nominator,
            sender_nonce,
        );

        let signature = sign(&staker.key_pair, &data_to_sign);
        return build_proof(&staker.account_id, &staker.relayer, signature)
    }

    #[test]
    fn succeeds_with_good_values() {
        let collator_1 = to_acc_id(1u64);
        let collator_2 = to_acc_id(2u64);
        let staker: Staker = Default::default();
        let initial_stake = 100;
        let nomination = 10;
        ExtBuilder::default()
            .with_balances(vec![
                (collator_1, 10000),
                (collator_2, 10000),
                (staker.account_id, 10000),
                (staker.relayer, 10000),
            ])
            .with_candidates(vec![(collator_1, initial_stake), (collator_2, initial_stake)])
            .with_nominations(vec![
                (staker.account_id, collator_1, nomination),
                (staker.account_id, collator_2, nomination),
            ])
            .build()
            .execute_with(|| {
                schedule_leave(staker.clone());

                // Roll foreward by "Delay" eras to activate leave
                roll_to_era_begin((ParachainStaking::delay() + 1u32) as u64);

                let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                let execute_leave_nomination_call = create_call_for_signed_execute_leave_nominators(
                    &staker,
                    nonce,
                    &staker.account_id,
                );

                assert_ok!(AvnProxy::proxy(
                    Origin::signed(staker.relayer),
                    execute_leave_nomination_call,
                    None
                ));

                assert_event_emitted!(Event::NominatorLeft {
                    nominator: staker.account_id,
                    unstaked_amount: nomination * 2,
                });

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
                    schedule_leave(staker.clone());

                    // Roll foreward by "Delay" eras to activate leave
                    roll_to_era_begin((ParachainStaking::delay() + 1u32) as u64);

                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);
                    let proof = create_proof_for_signed_execute_leave_nominators(
                        nonce,
                        &staker,
                        &staker.account_id,
                    );

                    assert_noop!(
                        ParachainStaking::signed_execute_leave_nominators(
                            RawOrigin::None.into(),
                            proof.clone(),
                            staker.account_id
                        ),
                        BadOrigin
                    );

                    // Show that we can send a successful transaction if its signed.
                    assert_ok!(ParachainStaking::signed_execute_leave_nominators(
                        Origin::signed(staker.account_id),
                        proof,
                        staker.account_id
                    ));
                });
        }

        #[test]
        fn proxy_proof_nonce_is_not_valid() {
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
                    schedule_leave(staker.clone());

                    // Roll foreward by "Delay" eras to activate leave
                    roll_to_era_begin((ParachainStaking::delay() + 1u32) as u64);

                    let bad_nonce = ParachainStaking::proxy_nonce(staker.account_id) + 1;
                    let execute_leave_nominators_call =
                        create_call_for_signed_execute_leave_nominators(
                            &staker,
                            bad_nonce,
                            &staker.account_id,
                        );

                    assert_noop!(
                        AvnProxy::proxy(
                            Origin::signed(staker.relayer),
                            execute_leave_nominators_call,
                            None
                        ),
                        Error::<Test>::UnauthorizedSignedExecuteLeaveNominatorsTransaction
                    );
                });
        }

        #[test]
        fn proxy_proof_nominator_is_not_valid() {
            let collator_1 = to_acc_id(1u64);
            let collator_2 = to_acc_id(2u64);
            let bad_nominator = to_acc_id(2000u64);
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
                    schedule_leave(staker.clone());

                    // Roll foreward by "Delay" eras to activate leave
                    roll_to_era_begin((ParachainStaking::delay() + 1u32) as u64);

                    let nonce = ParachainStaking::proxy_nonce(staker.account_id);

                    let proof = create_proof_for_signed_execute_leave_nominators(
                        nonce,
                        &staker,
                        &bad_nominator,
                    );

                    let execute_leave_nominators_call =
                        create_call_for_signed_execute_leave_nominators_from_proof(
                            proof,
                            &bad_nominator,
                        );

                    assert_noop!(
                        AvnProxy::proxy(
                            Origin::signed(staker.relayer),
                            execute_leave_nominators_call,
                            None
                        ),
                        Error::<Test>::NominatorDNE
                    );
                });
        }
    }
}
