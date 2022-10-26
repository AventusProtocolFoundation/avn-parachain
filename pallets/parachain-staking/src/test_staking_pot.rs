#[cfg(test)]
use crate::mock::*;
use frame_support::traits::Currency;

pub const ONE_TOKEN: u128 = 1_000000_000000_000000u128;
pub const AMOUNT_100_TOKEN: u128 = 100 * ONE_TOKEN;
pub const NON_COLLATOR_ACCOUNT_ID: u64 = 2u64;

fn get_total_balance_of_collators(collator_account_ids: &Vec<AccountId>) -> u128 {
    return collator_account_ids
        .clone()
        .into_iter()
        .map(|v| Balances::free_balance(v))
        .sum::<u128>()
}

#[test]
fn fee_is_added_to_pot() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .build()
        .execute_with(|| {
            let fee: u128 = (BASE_FEE + TX_LEN as u64) as u128;
            let sender = NON_COLLATOR_ACCOUNT_ID;
            Balances::make_free_balance_be(&sender, AMOUNT_100_TOKEN);

            let sender_balance = Balances::free_balance(sender);
            let staking_pot_balance = ParachainStaking::reward_pot();
            let total_collators_balance = get_total_balance_of_collators(&vec![1, 3, 4]);

            let no_tip = 0u128;
            pay_gas_for_transaction(&sender, no_tip);

            // Sender paid the transaction fee
            assert_eq!(Balances::free_balance(sender), sender_balance - fee);

            // Collator balances remain the same
            assert_eq!(get_total_balance_of_collators(&vec![1, 3, 4]), total_collators_balance);

            // New pot balance has increased
            assert_eq!(ParachainStaking::reward_pot(), staking_pot_balance + fee);
        });
}

#[test]
fn fee_is_accumulated_to_pot() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .build()
        .execute_with(|| {
            let fee: u128 = (BASE_FEE + TX_LEN as u64) as u128;
            let sender = NON_COLLATOR_ACCOUNT_ID;
            Balances::make_free_balance_be(&sender, AMOUNT_100_TOKEN);

            let sender_balance = Balances::free_balance(sender);
            let staking_pot_balance = ParachainStaking::reward_pot();
            let total_collators_balance = get_total_balance_of_collators(&vec![1, 3, 4]);

            let no_tip = 0u128;
            pay_gas_for_transaction(&sender, no_tip);

            // Simulate paying fees for a second transaction
            pay_gas_for_transaction(&sender, no_tip);

            // Sender paid the transaction fee
            assert_eq!(Balances::free_balance(sender), sender_balance - fee * 2);

            // Collator balances remain the same
            assert_eq!(get_total_balance_of_collators(&vec![1, 3, 4]), total_collators_balance);

            // New pot balance has increased again
            assert_eq!(ParachainStaking::reward_pot(), staking_pot_balance + fee * 2);
        });
}

#[test]
fn fee_and_tip_is_added_to_pot() {
    ExtBuilder::default()
        .with_balances(vec![(1, 20), (2, 40), (3, 20), (4, 20)])
        .with_candidates(vec![(1, 20), (3, 20), (4, 20)])
        .build()
        .execute_with(|| {
            let fee: u128 = (BASE_FEE + TX_LEN as u64) as u128;
            let sender = NON_COLLATOR_ACCOUNT_ID;
            let tip = 15u128;
            Balances::make_free_balance_be(&sender, AMOUNT_100_TOKEN);

            let sender_balance = Balances::free_balance(sender);
            let staking_pot_balance = ParachainStaking::reward_pot();
            let total_collators_balance = get_total_balance_of_collators(&vec![1, 3, 4]);

            pay_gas_for_transaction(&sender, tip);

            // Sender paid the transaction fee and tip
            assert_eq!(Balances::free_balance(sender), sender_balance - fee - tip);

            // Collator balances remain the same
            assert_eq!(get_total_balance_of_collators(&vec![1, 3, 4]), total_collators_balance);

            // New pot balance has increased
            assert_eq!(ParachainStaking::reward_pot(), staking_pot_balance + fee + tip);
        });
}
