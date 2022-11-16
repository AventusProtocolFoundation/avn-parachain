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


