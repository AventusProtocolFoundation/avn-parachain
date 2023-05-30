use super::*;
use crate::{
    mock::{
        TestAccount,
        AccountId,
        new_test_ext,
        TestRuntime,
        AvnTransactionPayment,
        RuntimeOrigin
    },
};

use frame_support::{
    assert_ok,
    assert_noop
};

fn to_acc_id(id: u64) -> AccountId {
    return TestAccount::new(id).account_id()
}

mod set_known_senders {
    use super::*;

    mod succeeds_when {
        use super::*;

        #[test]
        fn call_is_set_with_correct_pertencage_fee() {
            new_test_ext().execute_with(|| {
                let account_1 = to_acc_id(1u64);

                let config = AdjustmentInput::<TestRuntime> {
                    fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                        percentage: 10,
                        _marker: sp_std::marker::PhantomData::<TestRuntime>
                    }),
                    adjustment_type: AdjustmentType::None,
                };

                assert_ok!(AvnTransactionPayment::set_known_sender(
                    RuntimeOrigin::root(),
                    account_1,
                    config,
                ));
            })
        }

        #[test]
        fn call_is_set_with_correct_fixed_fee() {
            new_test_ext().execute_with(|| {
                let account_1 = to_acc_id(1u64);

                let config = AdjustmentInput::<TestRuntime> {
                    fee_type: FeeType::FixedFee(FixedFeeConfig {
                        fee: 10
                    }),
                    adjustment_type: AdjustmentType::None,
                };

                assert_ok!(AvnTransactionPayment::set_known_sender(
                    RuntimeOrigin::root(),
                    account_1,
                    config,
                ));
             })
        }

        #[test]
        fn call_is_set_with_correct_transaction_based_adjustment_type() {
            new_test_ext().execute_with(|| {
                let account_1 = to_acc_id(1u64);

                let config = AdjustmentInput::<TestRuntime> {
                    fee_type: FeeType::FixedFee(FixedFeeConfig {
                        fee: 1
                    }),
                    adjustment_type: AdjustmentType::TransactionBased(NumberOfTransactions {
                        number_of_transactions: 5
                    }),
                };

                assert_ok!(AvnTransactionPayment::set_known_sender(
                    RuntimeOrigin::root(),
                    account_1,
                    config,
                ));
             })
        }

        #[test]
        fn call_is_set_with_correct_time_based_adjustment_type() {
            new_test_ext().execute_with(|| {
                let account_1 = to_acc_id(1u64);

                let config = AdjustmentInput::<TestRuntime> {
                    fee_type: FeeType::FixedFee(FixedFeeConfig {
                        fee: 1
                    }),
                    adjustment_type: AdjustmentType::TimeBased(Duration {
                        duration: 1
                    }),
                };

                assert_ok!(AvnTransactionPayment::set_known_sender(
                    RuntimeOrigin::root(),
                    account_1,
                    config,
                ));
             })
        }
    }

    mod fails_when {
        use super::*;

        #[test]
        fn call_is_set_with_bad_pertencage_fee() {
            new_test_ext().execute_with(|| {
                let account_1 = to_acc_id(1u64);

                let config = AdjustmentInput::<TestRuntime> {
                    fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                        percentage: 0,
                        _marker: sp_std::marker::PhantomData::<TestRuntime>
                    }),
                    adjustment_type: AdjustmentType::None,
                };

                assert_noop!(AvnTransactionPayment::set_known_sender(
                    RuntimeOrigin::root(),
                    account_1,
                    config,
                ),
                Error::<TestRuntime>::InvalidFeeConfig);
             })
        }

        #[test]
        fn call_is_set_with_bad_fixed_fee() {
            new_test_ext().execute_with(|| {
                let account_1 = to_acc_id(1u64);

                let config = AdjustmentInput::<TestRuntime> {
                    fee_type: FeeType::FixedFee(FixedFeeConfig {
                        fee: 0
                    }),
                    adjustment_type: AdjustmentType::None,
                };

                assert_noop!(AvnTransactionPayment::set_known_sender(
                    RuntimeOrigin::root(),
                    account_1,
                    config,
                ),
                Error::<TestRuntime>::InvalidFeeConfig);
             })
        }

        #[test]
        fn call_is_set_with_bad_transaction_based_adjustment_type() {
            new_test_ext().execute_with(|| {
                let account_1 = to_acc_id(1u64);

                let config = AdjustmentInput::<TestRuntime> {
                    fee_type: FeeType::FixedFee(FixedFeeConfig {
                        fee: 1
                    }),
                    adjustment_type: AdjustmentType::TransactionBased(NumberOfTransactions {
                        number_of_transactions: 0
                    }),
                };

                assert_noop!(AvnTransactionPayment::set_known_sender(
                    RuntimeOrigin::root(),
                    account_1,
                    config,
                ),
                Error::<TestRuntime>::InvalidFeeConfig);
             })
        }

        #[test]
        fn call_is_set_with_bad_time_based_adjustment_type() {
            new_test_ext().execute_with(|| {
                let account_1 = to_acc_id(1u64);

                let config = AdjustmentInput::<TestRuntime> {
                    fee_type: FeeType::FixedFee(FixedFeeConfig {
                        fee: 1
                    }),
                    adjustment_type: AdjustmentType::TimeBased(Duration {
                        duration: 0
                    }),
                };

                assert_noop!(AvnTransactionPayment::set_known_sender(
                    RuntimeOrigin::root(),
                    account_1,
                    config,
                ),
                Error::<TestRuntime>::InvalidFeeConfig);
             })
        }
    }
}