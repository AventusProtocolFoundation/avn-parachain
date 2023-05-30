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

    #[test]
    fn correct_pertencage_fee() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                    percentage: 10,
                    _marker: sp_std::marker::PhantomData::<TestRuntime>
                }),
                adjustment_type: None,
            };

            assert_ok!(AvnTransactionPayment::set_known_sender(
                RuntimeOrigin::root(),
                account_1,
                config,
            ));
         })
    }

    #[test]
    fn bad_pertencage_fee() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::PercentageFee(PercentageFeeConfig {
                    percentage: 0,
                    _marker: sp_std::marker::PhantomData::<TestRuntime>
                }),
                adjustment_type: None,
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
    fn correct_fixed_fee() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::FixedFee(FixedFeeConfig {
                    fee: 10
                }),
                adjustment_type: None,
            };

            assert_ok!(AvnTransactionPayment::set_known_sender(
                RuntimeOrigin::root(),
                account_1,
                config,
            ));
         })
    }

    #[test]
    fn bad_fixed_fee() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::FixedFee(FixedFeeConfig {
                    fee: 0
                }),
                adjustment_type: None,
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

mod adjustement_types {
    use super::*;


    #[test]
    fn transaction_based() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::FixedFee(FixedFeeConfig {
                    fee: 1
                }),
                adjustment_type: Some(AdjustmentType::TransactionBased(5)),
            };

            assert_ok!(AvnTransactionPayment::set_known_sender(
                RuntimeOrigin::root(),
                account_1,
                config,
            ));
         })
    }

    #[test]
    fn bad_transaction_based() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::FixedFee(FixedFeeConfig {
                    fee: 1
                }),
                adjustment_type: Some(AdjustmentType::TransactionBased(0)),
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
    fn bad_fee_type() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::Unknown,
                adjustment_type: Some(AdjustmentType::TransactionBased(5)),
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
    fn time_based() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::FixedFee(FixedFeeConfig {
                    fee: 1
                }),
                adjustment_type: Some(AdjustmentType::TimeBased(5)),
            };

            assert_ok!(AvnTransactionPayment::set_known_sender(
                RuntimeOrigin::root(),
                account_1,
                config,
            ));
         })
    }

    #[test]
    fn bad_time_based() {
        new_test_ext().execute_with(|| {
            let account_1 = to_acc_id(1u64);

            let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::FixedFee(FixedFeeConfig {
                    fee: 1
                }),
                adjustment_type: Some(AdjustmentType::TimeBased(0)),
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