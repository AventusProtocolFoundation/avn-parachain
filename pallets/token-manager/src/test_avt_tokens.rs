// This file is part of Aventus.
// Copyright (C) 2022 Aventus Network Services (UK) Ltd.

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg(test)]
use crate::{
    mock::{Balances, RuntimeEvent, *},
    *,
};
use frame_support::{assert_noop, assert_ok};
use hex_literal::hex;
use pallet_balances::Error as BalancesError;

const USE_RECEIVER_WITH_EXISTING_AMOUNT: bool = true;
const USE_RECEIVER_WITH_0_AMOUNT: bool = false;

#[test]
fn avn_test_lift_to_zero_balance_account_should_succeed() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let mock_data = MockData::setup(AMOUNT_123_TOKEN, USE_RECEIVER_WITH_0_AMOUNT);
        let mock_event = &mock_data.avt_token_lift_event;
        insert_to_mock_processed_events(&mock_event.event_id);

        // check that TokenManager.balance for AVT contract is 0
        assert_eq!(TokenManager::balance((AVT_TOKEN_CONTRACT, mock_data.receiver_account_id)), 0);

        assert_eq!(Balances::free_balance(mock_data.receiver_account_id), 0);
        assert_ok!(TokenManager::lift(&mock_event));
        assert_eq!(Balances::free_balance(mock_data.receiver_account_id), AMOUNT_123_TOKEN);

        // check that TokenManager.balance for AVT contract is still 0
        assert_eq!(TokenManager::balance((AVT_TOKEN_CONTRACT, mock_data.receiver_account_id)), 0);

        assert!(System::events().iter().any(|a| a.event ==
            RuntimeEvent::TokenManager(crate::Event::<TestRuntime>::AVTLifted {
                recipient: mock_data.receiver_account_id,
                amount: AMOUNT_123_TOKEN,
                eth_tx_hash: mock_event.event_id.transaction_hash
            })));
    });
}

#[test]
fn avn_test_lift_to_non_zero_balance_account_should_succeed() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let mock_data = MockData::setup(AMOUNT_123_TOKEN, USE_RECEIVER_WITH_EXISTING_AMOUNT);
        let mock_event = &mock_data.avt_token_lift_event;
        insert_to_mock_processed_events(&mock_event.event_id);

        // check that TokenManager.balance for AVT contract is 0
        assert_eq!(TokenManager::balance((AVT_TOKEN_CONTRACT, mock_data.receiver_account_id)), 0);

        assert_eq!(Balances::free_balance(mock_data.receiver_account_id), AMOUNT_100_TOKEN);
        let new_balance = Balances::free_balance(mock_data.receiver_account_id) + AMOUNT_123_TOKEN;

        assert_ok!(TokenManager::lift(&mock_event));
        assert_eq!(Balances::free_balance(mock_data.receiver_account_id), new_balance);

        // check that TokenManager.balance for AVT contract is still 0
        assert_eq!(TokenManager::balance((AVT_TOKEN_CONTRACT, mock_data.receiver_account_id)), 0);

        assert!(System::events().iter().any(|a| a.event ==
            RuntimeEvent::TokenManager(crate::Event::<TestRuntime>::AVTLifted {
                recipient: mock_data.receiver_account_id,
                amount: AMOUNT_123_TOKEN,
                eth_tx_hash: mock_event.event_id.transaction_hash
            })));
    });
}

#[test]
fn avn_test_lift_max_balance_to_zero_balance_account_should_succeed() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let u128_max_amount: u128 = u128::max_value();
        let mock_data = MockData::setup(u128_max_amount, USE_RECEIVER_WITH_0_AMOUNT);
        let mock_event = &mock_data.avt_token_lift_event;
        insert_to_mock_processed_events(&mock_event.event_id);

        assert_eq!(Balances::free_balance(mock_data.receiver_account_id), 0);
        assert_ok!(TokenManager::lift(&mock_event));
        assert_eq!(Balances::free_balance(mock_data.receiver_account_id), u128_max_amount);

        assert!(System::events().iter().any(|a| a.event ==
            RuntimeEvent::TokenManager(crate::Event::<TestRuntime>::AVTLifted {
                recipient: mock_data.receiver_account_id,
                amount: u128_max_amount,
                eth_tx_hash: mock_event.event_id.transaction_hash
            })));
    });
}

#[test]
fn avn_test_lift_max_balance_to_non_zero_balance_account_should_return_deposit_failed_error() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let u128_max_amount = u128::max_value();
        let mock_data = MockData::setup(u128_max_amount, USE_RECEIVER_WITH_EXISTING_AMOUNT);
        let mock_event = &mock_data.avt_token_lift_event;
        insert_to_mock_processed_events(&mock_event.event_id);
        let balance_before = Balances::free_balance(mock_data.receiver_account_id);

        assert_noop!(TokenManager::lift(&mock_event), Error::<TestRuntime>::DepositFailed);
        assert_eq!(Balances::free_balance(mock_data.receiver_account_id), balance_before);

        assert!(!System::events().iter().any(|a| a.event ==
            RuntimeEvent::TokenManager(crate::Event::<TestRuntime>::AVTLifted {
                recipient: mock_data.receiver_account_id,
                amount: u128_max_amount,
                eth_tx_hash: mock_event.event_id.transaction_hash
            })));
    });
}

#[test]
fn avn_test_lower_all_avt_token_succeed() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let (_, from_account_id, to_account_id, t1_recipient) =
            MockData::setup_lower_request_data();
        let from_account_balance_before = Balances::free_balance(from_account_id);
        let amount = from_account_balance_before;

        assert_ok!(TokenManager::lower(
            RuntimeOrigin::signed(from_account_id),
            from_account_id,
            AVT_TOKEN_CONTRACT,
            amount,
            t1_recipient
        ));
        assert_eq!(Balances::free_balance(from_account_id), from_account_balance_before - amount);
        assert!(System::events().iter().any(|a| a.event ==
            RuntimeEvent::Balances(pallet_balances::Event::<TestRuntime>::Withdraw { 
                who: from_account_id, 
                amount: amount
            })));
    });
}

#[test]
fn avn_test_lower_some_avt_token_succeed() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let (_, from_account_id, to_account_id, t1_recipient) =
            MockData::setup_lower_request_data();
        let from_account_balance_before = Balances::free_balance(from_account_id);
        let amount = from_account_balance_before / 2;

        assert_ok!(TokenManager::lower(
            RuntimeOrigin::signed(from_account_id),
            from_account_id,
            AVT_TOKEN_CONTRACT,
            amount,
            t1_recipient
        ));
        assert_eq!(Balances::free_balance(from_account_id), from_account_balance_before - amount);
        assert!(System::events().iter().any(|a| a.event ==
            RuntimeEvent::Balances(pallet_balances::Event::<TestRuntime>::Withdraw { 
                who: from_account_id, 
                amount: amount 
            })));
    });
}

#[test]
fn avn_test_lower_avt_token_should_fail_when_sender_does_not_have_enough_avt_token() {
    let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();

    ext.execute_with(|| {
        let (_, _, _, t1_recipient) = MockData::setup_lower_request_data();
        let from_account = H256::random();
        let from_account_id =
            <TestRuntime as frame_system::Config>::AccountId::decode(&mut from_account.as_bytes())
                .unwrap();
        let amount = 1;

        assert_noop!(
            TokenManager::lower(
                RuntimeOrigin::signed(from_account_id),
                from_account_id,
                AVT_TOKEN_CONTRACT,
                amount,
                t1_recipient
            ),
            BalancesError::<TestRuntime, _>::InsufficientBalance
        );
    });
}

// Note: This test prevents the implementation of lower function to use a t2 destination account to
// receive all the tokens which may cause an overflow of the t2 destination account token balance
#[test]
fn avn_test_avt_token_total_lowered_amount_greater_than_balance_max_value_ok() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let (_, from_account_id, to_account_id, _) = MockData::setup_lower_request_data();
        let mut from_account_balance_before = Balances::free_balance(from_account_id);
        let mut amount = from_account_balance_before;
        let t1_recipient = H160(hex!("0000000000000000000000000000000000000001"));

        assert_ok!(TokenManager::lower(
            RuntimeOrigin::signed(from_account_id),
            from_account_id,
            AVT_TOKEN_CONTRACT,
            amount,
            t1_recipient
        ));
        assert_eq!(Balances::free_balance(from_account_id), from_account_balance_before - amount);
        assert!(System::events().iter().any(|a| a.event ==
            RuntimeEvent::Balances(pallet_balances::Event::<TestRuntime>::Withdraw { 
                who: from_account_id, 
                amount: amount 
            })));

        // Lift and lower AVT tokens again
        amount = u128::max_value();
        MockData::set_avt_balance(from_account_id, amount);
        from_account_balance_before = Balances::free_balance(from_account_id);

        assert_ok!(TokenManager::lower(
            RuntimeOrigin::signed(from_account_id),
            from_account_id,
            AVT_TOKEN_CONTRACT,
            amount,
            t1_recipient
        ));
        assert_eq!(Balances::free_balance(from_account_id), from_account_balance_before - amount);
        assert!(System::events().iter().any(|a| a.event ==
            RuntimeEvent::Balances(pallet_balances::Event::<TestRuntime>::Withdraw { 
                who: from_account_id, 
                amount: amount 
            })));
    });
}
