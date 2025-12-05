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
use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};

mod burn_tests {
    use super::*;

    mod set_burn_period {
        use super::*;

        mod succeeds_when {
            use super::*;

            #[test]
            fn origin_is_sudo() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_balances()
                    .as_externality();

                ext.execute_with(|| {
                    let current_block: u64 = 1;
                    frame_system::Pallet::<TestRuntime>::set_block_number(current_block);

                    let new_period: u32 = 8000;
                    assert_ok!(TokenManager::set_burn_period(RuntimeOrigin::root(), new_period,));

                    // storage updated correctly
                    assert_eq!(BurnRefreshRange::<TestRuntime>::get(), new_period);

                    // next burn rescheduled from "now"
                    let expected_next: u64 = current_block + new_period as u64;
                    assert_eq!(NextBurnAt::<TestRuntime>::get(), expected_next);

                    // event emitted
                    assert!(event_emitted(&mock::RuntimeEvent::TokenManager(crate::Event::<
                        TestRuntime,
                    >::BurnPeriodUpdated {
                        burn_period: new_period
                    })));
                });
            }
        }

        mod fails_when {
            use super::*;

            #[test]
            fn origin_is_not_sudo() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_balances()
                    .as_externality();

                ext.execute_with(|| {
                    let current_block: u64 = 1;
                    frame_system::Pallet::<TestRuntime>::set_block_number(current_block);

                    let new_period: u32 = 8000;
                    assert_noop!(
                        TokenManager::set_burn_period(
                            RuntimeOrigin::signed(account_id_with_seed_item(1)),
                            new_period,
                        ),
                        sp_runtime::DispatchError::BadOrigin,
                    );
                });
            }

            #[test]
            fn burn_period_is_below_minimum() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_balances()
                    .as_externality();

                ext.execute_with(|| {
                    let current_block: u64 = 1;
                    frame_system::Pallet::<TestRuntime>::set_block_number(current_block);

                    let min_burn_period =
                        <TestRuntime as crate::Config>::MinBurnRefreshRange::get();
                    let invalid_period = min_burn_period.saturating_sub(1);
                    assert_noop!(
                        TokenManager::set_burn_period(RuntimeOrigin::root(), invalid_period,),
                        Error::<TestRuntime>::InvalidBurnPeriod,
                    );
                })
            }
        }
    }

    mod on_initialize {
        use super::*;

        mod succeeds_when {
            use super::*;

            #[test]
            fn burn_is_due_and_burn_pot_is_not_empty() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_balances()
                    .as_externality();

                ext.execute_with(|| {
                    let current_block: u64 = 1;
                    frame_system::Pallet::<TestRuntime>::set_block_number(current_block);

                    // Burn is due at this block
                    NextBurnAt::<TestRuntime>::put(current_block);

                    // Put funds into burn pot
                    let burn_pot = TokenManager::burn_pot_account();
                    let amount = 1_000u128;
                    pallet_balances::Pallet::<TestRuntime>::make_free_balance_be(&burn_pot, amount);

                    // call hook
                    TokenManager::on_initialize(current_block);

                    // event emitted
                    assert!(event_emitted(&mock::RuntimeEvent::TokenManager(crate::Event::<
                        TestRuntime,
                    >::BurnedFromPot {
                        amount
                    })));
                });
            }
        }

        mod fails_when {
            use super::*;

            #[test]
            fn burn_is_not_due() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_balances()
                    .as_externality();

                ext.execute_with(|| {
                    let current_block: u64 = 1;
                    frame_system::Pallet::<TestRuntime>::set_block_number(current_block);

                    // Not due yet
                    NextBurnAt::<TestRuntime>::put(current_block + 10);

                    // Put funds into burn pot
                    let burn_pot = TokenManager::burn_pot_account();
                    let amount = 1_000u128;
                    pallet_balances::Pallet::<TestRuntime>::make_free_balance_be(&burn_pot, amount);

                    // call hook
                    TokenManager::on_initialize(current_block);

                    // event NOT emitted
                    assert!(!event_emitted(&mock::RuntimeEvent::TokenManager(crate::Event::<
                        TestRuntime,
                    >::BurnedFromPot {
                        amount
                    })));
                });
            }

            #[test]
            fn burn_is_due_but_burn_pot_is_empty() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_balances()
                    .as_externality();

                ext.execute_with(|| {
                    let current_block: u64 = 1;
                    frame_system::Pallet::<TestRuntime>::set_block_number(current_block);

                    // Due
                    NextBurnAt::<TestRuntime>::put(current_block);

                    // Empty burn pot
                    let burn_pot = TokenManager::burn_pot_account();
                    let amount = 0u128;
                    pallet_balances::Pallet::<TestRuntime>::make_free_balance_be(&burn_pot, amount);

                    // call hook
                    TokenManager::on_initialize(current_block);

                    // event NOT emitted (because amount is zero)
                    assert!(!event_emitted(&mock::RuntimeEvent::TokenManager(crate::Event::<
                        TestRuntime,
                    >::BurnedFromPot {
                        amount
                    })));
                });
            }
        }
    }
}
