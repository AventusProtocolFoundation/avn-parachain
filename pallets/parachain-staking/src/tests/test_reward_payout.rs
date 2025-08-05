#[cfg(test)]
use crate::mock::{
    pay_gas_for_transaction, roll_one_block, roll_to_era_begin, set_author, AccountId, Balances,
    ExtBuilder, ParachainStaking, TestAccount, BASE_FEE, TX_LEN,
};
use crate::{assert_eq_events, assert_event_emitted, Event};
use frame_support::traits::Currency;
use sp_runtime::{traits::Zero, Perbill};

fn collator_1() -> AccountId {
    return TestAccount::new(1u64).account_id()
}

fn collator_2() -> AccountId {
    return TestAccount::new(2u64).account_id()
}

fn tx_sender() -> AccountId {
    return TestAccount::new(3u64).account_id()
}

fn nominator() -> AccountId {
    return TestAccount::new(4u64).account_id()
}

const ERA_BLOCKS_HAVE_BEEN_AUTHORED: u32 = 1;
const TIP: u128 = 5;
const COLLATOR1_POINTS: u32 = 4;
const COLLATOR2_POINTS: u32 = 2;
const NOMINATOR4_STAKE: u128 = 500;
const COLLATOR1_OWN_STAKE: u128 = 1000;
const COLLATOR1_TOTAL_STAKE: u128 = COLLATOR1_OWN_STAKE + NOMINATOR4_STAKE;
const COLLATOR2_OWN_STAKE: u128 = 500;
const TOTAL_STAKE: u128 = COLLATOR1_OWN_STAKE + COLLATOR2_OWN_STAKE + NOMINATOR4_STAKE;
const TOTAL_POINTS_FOR_ERA: u32 = COLLATOR1_POINTS + COLLATOR2_POINTS;

fn expected_tx_fee() -> u128 {
    return (BASE_FEE + TX_LEN as u64) as u128
}

// This function will setup the payments so both collators get the same reward
fn set_reward_pot_and_trigger_payout(block_author_era: u32, destination_era: u64) -> (u128, u128) {
    pay_gas_for_transaction(&tx_sender(), TIP);

    set_author(block_author_era, collator_1(), COLLATOR1_POINTS);
    set_author(block_author_era, collator_2(), COLLATOR2_POINTS);

    roll_to_era_begin(destination_era);

    let expected_total_reward = expected_tx_fee() + TIP;

    let collator1_points_percentage =
        Perbill::from_rational(COLLATOR1_POINTS, TOTAL_POINTS_FOR_ERA);
    let collator1_total_reward = collator1_points_percentage * expected_total_reward;

    let collator2_points_percentage =
        Perbill::from_rational(COLLATOR2_POINTS, TOTAL_POINTS_FOR_ERA);
    let collator2_total_reward = collator2_points_percentage * expected_total_reward;

    return (collator1_total_reward, collator2_total_reward)
}
