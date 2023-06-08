use super::*;
use crate::mock::{
    new_test_ext, roll_one_block, AccountId, Balances, RuntimeCall, RuntimeOrigin, System,
    TestAccount, TestRuntime, event_emitted, RuntimeEvent
};

use frame_support::{dispatch::DispatchInfo, pallet_prelude::Weight};
use pallet_transaction_payment::ChargeTransactionPayment;
use sp_runtime::traits::SignedExtension;

use frame_support::assert_ok;

pub const TX_LEN: usize = 1;
pub const INITIAL_SENDER_BALANCE: u128 = 200;
pub const BASE_FEE: u32 = 14;
pub const FIXED_FEE: u128 = 10;
pub const PERCENTAGE_FEE: u32 = 50;

fn to_acc_id(id: u64) -> AccountId {
    return TestAccount::new(id).account_id()
}

/// create a transaction info struct from weight. Handy to avoid building the whole struct.
pub fn info_from_weight(w: Weight) -> DispatchInfo {
    DispatchInfo { weight: w, ..Default::default() }
}

fn pay_gas_and_call_remark(sender: &AccountId) {
    let pre = <ChargeTransactionPayment<TestRuntime> as SignedExtension>::pre_dispatch(
        ChargeTransactionPayment::from(0),
        sender,
        &RuntimeCall::System(frame_system::Call::remark { remark: vec![] }),
        &info_from_weight(Weight::from_ref_time(1)),
        TX_LEN,
    );

    assert_ok!(&pre);

    assert_ok!(System::remark(RuntimeOrigin::signed(*sender), vec![])
        .map_err(|_e| Error::<TestRuntime>::InvalidFeeConfig));

    assert_ok!(ChargeTransactionPayment::<TestRuntime>::post_dispatch(
        Some(pre.expect("Checked for error")),
        &DispatchInfo { weight: Weight::from_ref_time(1), ..Default::default() },
        &PostDispatchInfo { actual_weight: None, pays_fee: Default::default() },
        TX_LEN,
        &Ok(())
    ));

    System::inc_account_nonce(sender);
}

fn set_initial_sender_balance(sender: &AccountId) {
    Balances::make_free_balance_be(&sender, INITIAL_SENDER_BALANCE);
    assert_eq!(INITIAL_SENDER_BALANCE, Balances::free_balance(sender));
}

fn get_percentage_fee_value(percentage_fee: u32) -> u32 {
    (BASE_FEE * percentage_fee) / 100
}

pub(crate) fn fee_adjusted_event_emitted() -> bool {
    System::events()
        .into_iter()
        .map(|r| r.event)
        .filter_map(
            |e| if let RuntimeEvent::AvnTransactionPayment(inner) = e { Some(inner) } else { None },
        )
        .collect::<Vec<_>>()
        .len() > 0
}

/// Rolls desired block number of times.
pub(crate) fn roll_blocks(n: u64) {
    let mut block = System::block_number();
    let target_block = block + n;
    while block < target_block {
        block = roll_one_block();
    }
}

mod discount_tests {
    use super::*;

    mod fees_are_paid_correctly_when {
        use super::*;

        mod call_is_set_without_adjustment_type {
            use super::*;
            use crate::mock::AvnTransactionPayment;

            #[test]
            fn and_valid_fixed_fee() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::FixedFee(FixedFeeConfig { fee: FIXED_FEE }),
                        adjustment_type: AdjustmentType::None,
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    assert_eq!(INITIAL_SENDER_BALANCE - FIXED_FEE, Balances::free_balance(sender));

                    assert!(event_emitted(&mock::RuntimeEvent::AvnTransactionPayment(
                        crate::Event::<TestRuntime>::AdjustedTransactionFeePaid {
                            who: sender,
                            fee: FIXED_FEE,
                        }
                    )));
                })
            }

            #[test]
            fn and_fixed_fee_higher_than_base_fee() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let higher_fixed_fee = u128::from(BASE_FEE + 1);
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::FixedFee(FixedFeeConfig { fee: higher_fixed_fee }),
                        adjustment_type: AdjustmentType::None,
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE,
                        Balances::free_balance(sender)
                    );

                    assert!(event_emitted(&mock::RuntimeEvent::AvnTransactionPayment(
                        crate::Event::<TestRuntime>::AdjustedTransactionFeePaid {
                            who: sender,
                            fee: 0u128, // sender got 100% refund
                        }
                    )));
                })
            }

            #[test]
            fn and_valid_percentage_fee() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                            percentage: PERCENTAGE_FEE,
                            _marker: sp_std::marker::PhantomData::<TestRuntime>,
                        }),
                        adjustment_type: AdjustmentType::None,
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);

                    let percentage_value = get_percentage_fee_value(PERCENTAGE_FEE);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(percentage_value),
                        Balances::free_balance(sender)
                    );

                    assert!(event_emitted(&mock::RuntimeEvent::AvnTransactionPayment(
                        crate::Event::<TestRuntime>::AdjustedTransactionFeePaid {
                            who: sender,
                            fee: percentage_value as u128
                        }
                    )));
                })
            }

            #[test]
            fn and_percentage_fee_is_100() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let high_percentage_fee = 100;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                            percentage: high_percentage_fee,
                            _marker: sp_std::marker::PhantomData::<TestRuntime>,
                        }),
                        adjustment_type: AdjustmentType::None,
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE,
                        Balances::free_balance(sender)
                    );

                    assert!(event_emitted(&mock::RuntimeEvent::AvnTransactionPayment(
                        crate::Event::<TestRuntime>::AdjustedTransactionFeePaid {
                            who: sender,
                            fee: 0u128
                        }
                    )));
                })
            }
        }

        mod call_is_set_with_valid_transaction_based_adjustment_type {
            use super::*;
            use crate::mock::AvnTransactionPayment;

            #[test]
            fn and_valid_fixed_fee() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let number_of_transactions = 1;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::FixedFee(FixedFeeConfig { fee: FIXED_FEE }),
                        adjustment_type: AdjustmentType::TransactionBased(NumberOfTransactions {
                            number_of_transactions,
                        }),
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    assert_eq!(INITIAL_SENDER_BALANCE - FIXED_FEE, Balances::free_balance(sender));

                    assert_eq!(fee_adjusted_event_emitted(), true);
                    assert!(event_emitted(&mock::RuntimeEvent::AvnTransactionPayment(
                        crate::Event::<TestRuntime>::AdjustedTransactionFeePaid {
                            who: sender,
                            fee: FIXED_FEE,
                        }
                    )));

                    <frame_system::Pallet<TestRuntime>>::reset_events();

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(BASE_FEE),
                        Balances::free_balance(sender)
                    );
                    assert_eq!(fee_adjusted_event_emitted(), false);
                })
            }

            #[test]
            fn and_valid_percentage_fee() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let number_of_transactions = 1;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                            percentage: PERCENTAGE_FEE,
                            _marker: sp_std::marker::PhantomData::<TestRuntime>,
                        }),
                        adjustment_type: AdjustmentType::TransactionBased(NumberOfTransactions {
                            number_of_transactions,
                        }),
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    let percentage_value = get_percentage_fee_value(PERCENTAGE_FEE);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(percentage_value),
                        Balances::free_balance(sender)
                    );

                    <frame_system::Pallet<TestRuntime>>::reset_events();

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(BASE_FEE),
                        Balances::free_balance(sender)
                    );
                    assert_eq!(fee_adjusted_event_emitted(), false);
                })
            }

            #[test]
            fn and_initial_sender_nonce_is_not_zero() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);

                    System::inc_account_nonce(sender);

                    set_initial_sender_balance(&sender);

                    let number_of_transactions = 1;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                            percentage: PERCENTAGE_FEE,
                            _marker: sp_std::marker::PhantomData::<TestRuntime>,
                        }),
                        adjustment_type: AdjustmentType::TransactionBased(NumberOfTransactions {
                            number_of_transactions,
                        }),
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    let percentage_value = get_percentage_fee_value(PERCENTAGE_FEE);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(percentage_value),
                        Balances::free_balance(sender)
                    );

                    <frame_system::Pallet<TestRuntime>>::reset_events();

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(BASE_FEE),
                        Balances::free_balance(sender)
                    );
                    assert_eq!(fee_adjusted_event_emitted(), false);
                })
            }
        }

        mod call_is_set_with_valid_block_based_adjustment_type {
            use super::*;
            use crate::mock::AvnTransactionPayment;

            #[test]
            fn and_valid_fixed_fee() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let duration = 1;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::FixedFee(FixedFeeConfig { fee: FIXED_FEE }),
                        adjustment_type: AdjustmentType::TimeBased(Duration { duration }),
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    assert_eq!(INITIAL_SENDER_BALANCE - FIXED_FEE, Balances::free_balance(sender));

                    roll_one_block();

                    <frame_system::Pallet<TestRuntime>>::reset_events();

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(BASE_FEE),
                        Balances::free_balance(sender)
                    );
                    assert_eq!(fee_adjusted_event_emitted(), false);
                })
            }

            #[test]
            fn and_valid_percentage_fee() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let duration = 1;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                            percentage: PERCENTAGE_FEE,
                            _marker: sp_std::marker::PhantomData::<TestRuntime>,
                        }),
                        adjustment_type: AdjustmentType::TimeBased(Duration { duration }),
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    let percentage_value = get_percentage_fee_value(PERCENTAGE_FEE);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(percentage_value),
                        Balances::free_balance(sender)
                    );

                    roll_one_block();

                    <frame_system::Pallet<TestRuntime>>::reset_events();

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(BASE_FEE),
                        Balances::free_balance(sender)
                    );
                    assert_eq!(fee_adjusted_event_emitted(), false);
                })
            }

            #[test]
            fn and_can_do_multiple_transactions_with_adjusted_fee_in_one_block() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let duration = 1;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::FixedFee(FixedFeeConfig { fee: FIXED_FEE }),
                        adjustment_type: AdjustmentType::TimeBased(Duration { duration }),
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);
                    assert_eq!(INITIAL_SENDER_BALANCE - FIXED_FEE, Balances::free_balance(sender));

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(INITIAL_SENDER_BALANCE - FIXED_FEE, Balances::free_balance(sender));

                    roll_one_block();

                    <frame_system::Pallet<TestRuntime>>::reset_events();

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(BASE_FEE),
                        Balances::free_balance(sender)
                    );
                    assert_eq!(fee_adjusted_event_emitted(), false);
                })
            }

            #[test]
            fn and_initial_block_is_not_zero() {
                new_test_ext().execute_with(|| {
                    roll_one_block();
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let duration = 1;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                            percentage: PERCENTAGE_FEE,
                            _marker: sp_std::marker::PhantomData::<TestRuntime>,
                        }),
                        adjustment_type: AdjustmentType::TimeBased(Duration { duration }),
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);

                    let percentage_value = get_percentage_fee_value(PERCENTAGE_FEE);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(percentage_value),
                        Balances::free_balance(sender)
                    );

                    roll_one_block();

                    <frame_system::Pallet<TestRuntime>>::reset_events();

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(BASE_FEE),
                        Balances::free_balance(sender)
                    );
                    assert_eq!(fee_adjusted_event_emitted(), false);
                })
            }

            #[test]
            fn and_a_high_duration() {
                new_test_ext().execute_with(|| {
                    let sender = to_acc_id(1u64);
                    set_initial_sender_balance(&sender);

                    let duration = 5;
                    let config = AdjustmentInput::<TestRuntime> {
                        fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                            percentage: PERCENTAGE_FEE,
                            _marker: sp_std::marker::PhantomData::<TestRuntime>,
                        }),
                        adjustment_type: AdjustmentType::TimeBased(Duration { duration }),
                    };

                    assert_ok!(AvnTransactionPayment::set_known_sender(
                        RuntimeOrigin::root(),
                        sender,
                        config,
                    ));
                    assert_eq!(AvnTransactionPayment::is_known_sender(sender), true);

                    pay_gas_and_call_remark(&sender);

                    let percentage_value = get_percentage_fee_value(PERCENTAGE_FEE);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(percentage_value),
                        Balances::free_balance(sender)
                    );

                    roll_blocks(5);

                    <frame_system::Pallet<TestRuntime>>::reset_events();

                    set_initial_sender_balance(&sender);
                    pay_gas_and_call_remark(&sender);
                    assert_eq!(
                        INITIAL_SENDER_BALANCE - u128::from(BASE_FEE),
                        Balances::free_balance(sender)
                    );
                    assert_eq!(fee_adjusted_event_emitted(), false);
                })
            }
        }
    }
}
