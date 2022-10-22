#[cfg(test)]

use crate::mock::{
    roll_one_block, roll_to_era_begin, set_author, Balances,
    ExtBuilder, ParachainStaking, Test, BASE_FEE, TX_LEN, AccountId, Call, pay_gas_for_transaction
};
use crate::{assert_eq_events, assert_event_emitted, Event};
use frame_support::{assert_ok, weights::{ DispatchInfo, PostDispatchInfo}};
use sp_runtime::{traits::Zero,};
use pallet_transaction_payment::ChargeTransactionPayment;
use sp_runtime::traits::SignedExtension;

#[test]
fn end_to_end_happy_path() {
    let collator1 = 1;
    let collator2 = 2;
    let nominator4 = 4;
    let tx_sender = 3;
    let era_blocks_have_been_authored = 1;
    let expected_tx_fee: u128 = (BASE_FEE + TX_LEN as u64) as u128;
    let tip = 5;
    let nominator4_stake = 500;
    let collator1_own_stake = 1000;
    let collator1_total_stake = collator1_own_stake + nominator4_stake;
    let collator2_own_stake = 500;
    let total_stake = collator1_own_stake + collator2_own_stake + nominator4_stake;
    let reward_pot_account_id = ParachainStaking::compute_reward_pot_account_id();

    ExtBuilder::default()
        .with_balances(vec![(collator1, 10000), (collator2, 10000), (tx_sender, 10000), (nominator4, 10000),])
        .with_candidates(vec![(collator1, collator1_own_stake), (collator2, collator2_own_stake)])
        .with_nominations(vec![(nominator4, collator1, nominator4_stake)])
        .build()
        .execute_with(|| {
            // Move to the begining of era 2.
            roll_to_era_begin(2);

            // To earn rewards:
            //   - Collators have to earn points for producing blocks
            //   - Reward pot must have some funds
            //   - 2 eras must have passed

            // Show that reward pot is empty
            assert_eq!(Balances::free_balance(&ParachainStaking::compute_reward_pot_account_id()), 0);
            assert_eq!(ParachainStaking::locked_era_payout(), 0);

            // Show a list of events we expect when rolling to era 2. Note the absence of rewards.
            let expected_events = vec![
                Event::CollatorChosen {era: 2, collator_account: collator1, total_exposed_amount: collator1_total_stake},
                Event::CollatorChosen {era: 2, collator_account: collator2, total_exposed_amount: collator2_own_stake},
                Event::NewEra {starting_block: 5, era: 2, selected_collators_number: 2, total_balance: total_stake},
            ];
            assert_eq_events!(expected_events);

            // We now set the conditions to start generating rewards
            //-------------------------------------------------------

            // Sending a transaction (with tip) to generate income for the chain
            pay_gas_for_transaction(&tx_sender, tip);

            let reward_pot_balance_before_reward_payout = Balances::free_balance(&reward_pot_account_id);

            // Assign block author points to collators 1 & 2.
            // Because it takes 2 eras before we can pay collators, we set the block authorship points for era 1.
            set_author(era_blocks_have_been_authored, collator1, 1);
            set_author(era_blocks_have_been_authored, collator2, 1);

            // We expect reward payouts on era 3 because all 3 conditions for earning rewards are met.
            roll_to_era_begin(3);

            // We now do the relevant checks
            //-------------------------------------------------------

            let total_reward_per_collator = (expected_tx_fee + tip) / 2; //divide by 2 because both collators earned 1 point each
            let expected_collator1_reward = (total_reward_per_collator * collator1_own_stake) / collator1_total_stake;
            let expected_nominator_reward = (total_reward_per_collator * nominator4_stake) / collator1_total_stake;

            assert_event_emitted!(Event::Rewarded {account: collator1, rewards: expected_collator1_reward});
            assert_event_emitted!(Event::Rewarded {account: nominator4, rewards: expected_nominator_reward});

            // Show that reward pot balance has decreased by "total reward payment amount"
            assert_eq!(
                Balances::free_balance(&reward_pot_account_id),
                reward_pot_balance_before_reward_payout - (expected_collator1_reward + expected_nominator_reward)
            );

            // Show that locked era payout has reserved enough to pay collator2
            assert_eq!(
                ParachainStaking::locked_era_payout(),
                (expected_tx_fee + tip) - (expected_collator1_reward + expected_nominator_reward)
            );

            // Move to the next block to trigger paying out the next collator
            roll_one_block();

            let expected_collator2_reward = total_reward_per_collator; //solo collator
            assert_event_emitted!(Event::Rewarded {account: collator2, rewards: expected_collator2_reward});

            // Show that reward pot balance and locked era balance are 0 because everything has been paid out for all collators
            assert_eq!(Balances::free_balance(&reward_pot_account_id), 0);
            assert_eq!(ParachainStaking::locked_era_payout(), 0);

            // check that distributing rewards clears awarded points
            assert!(ParachainStaking::awarded_pts(era_blocks_have_been_authored, collator1).is_zero());
        });
}

// TODO: add failing tests for payout logic (next PR)