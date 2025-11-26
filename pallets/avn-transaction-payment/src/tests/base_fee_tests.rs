use super::*;
use crate::mock::{
    event_emitted, new_test_ext, AccountId, AvnTransactionPayment, RuntimeOrigin, TestAccount,
    TestRuntime,
};
use frame_support::{assert_noop, assert_ok};

fn to_acc_id(id: u64) -> AccountId {
    return TestAccount::new(id).account_id()
}

mod base_fee_tests {
    use super::*;

    mod set_base_fee_usd {
        use super::*;

        mod succeeds_when {
            use super::*;

            #[test]
            fn origin_is_sudo() {
                new_test_ext().execute_with(|| {
                    let new_fee: u128 = 11_000_000; // $0.11 with 8 decimals
                    assert_ok!(AvnTransactionPayment::set_base_gas_fee_usd(
                        RuntimeOrigin::root(),
                        new_fee,
                    ));

                    // storage updated
                    assert_eq!(BaseGasFeeUsd::<TestRuntime>::get(), new_fee);

                    // event emitted
                    assert!(event_emitted(&mock::RuntimeEvent::AvnTransactionPayment(
                        crate::Event::<TestRuntime>::BaseGasFeeUsdSet { new_base_gas_fee: new_fee }
                    )));
                })
            }
        }

        mod fails_when {
            use super::*;

            #[test]
            fn origin_is_not_sudo() {
                new_test_ext().execute_with(|| {
                    let new_fee: u128 = 11_000_000; // $0.11 with 8 decimals
                    assert_noop!(
                        AvnTransactionPayment::set_base_gas_fee_usd(
                            RuntimeOrigin::signed(to_acc_id(1)),
                            new_fee,
                        ),
                        sp_runtime::DispatchError::BadOrigin,
                    );
                })
            }

            #[test]
            fn new_base_fee_is_zero() {
                new_test_ext().execute_with(|| {
                    let new_fee: u128 = 0;
                    assert_noop!(
                        AvnTransactionPayment::set_base_gas_fee_usd(
                            RuntimeOrigin::root(),
                            new_fee,
                        ),
                        Error::<TestRuntime>::BaseGasFeeZero,
                    );
                })
            }
        }
    }

    mod usd_min_fee_function {
        use super::*;

        mod succeeds_when {
            use super::*;
            #[test]
            fn returns_zero_when_base_fee_is_zero() {
                new_test_ext().execute_with(|| {
                    BaseGasFeeUsd::<TestRuntime>::put(0u128);

                    let min_fee = AvnTransactionPayment::usd_min_fee();

                    assert_eq!(min_fee, 0u128);
                });
            }

            #[test]
            fn computes_min_fee_from_base_usd_and_price() {
                new_test_ext().execute_with(|| {
                    // given:
                    // base_gas_fee_usd = 50_000_000 ($0.50 with 8 decimals) (minimum gas fee set by
                    // user) NativeRateProvider (in mock) returns price =
                    // 25_000_000 ($0.25) ( 1 AVT worth 0.25 returned by oracle)
                    // => usd_min_fee = 0.50 / 0.25 = 2 AVT (minimum calculated fee based on USD)

                    BaseGasFeeUsd::<TestRuntime>::put(50_000_000u128);

                    let min_fee = AvnTransactionPayment::usd_min_fee();

                    assert_eq!(min_fee, 2u128);
                });
            }
        }
    }

    mod fee_calculation_tests {
        use super::*;
        use codec::Encode;
        use pallet_transaction_payment::Pallet as TransactionPayment;

        #[test]
        fn simple_transfer_fee_is_at_least_usd_min_fee() {
            new_test_ext().execute_with(|| {
                // Arrange:
                // Set base fee USD and rely on TestRateProvider price.
                // For example:
                //   BaseGasFeeUsd = 5_000_000_000 ($50.00)
                //   price = 25_000_000 ($0.25)
                // => usd_min_fee = 200 Native tokens
                BaseGasFeeUsd::<TestRuntime>::put(5_000_000_000u128);

                // A simple transfer call
                let call = pallet_balances::Call::<TestRuntime>::transfer_allow_death {
                    dest: to_acc_id(2),
                    value: 1_000_000_000_000,
                };

                let info = call.get_dispatch_info();
                let len = call.encode().len() as u32;

                // Compute the fee as transaction-payment would
                let fee = TransactionPayment::<TestRuntime>::compute_fee(len, &info, 0u128.into());
                let min_fee = AvnTransactionPayment::usd_min_fee() as u128;

                // Fee is atleast min_fee
                assert!(
                    fee >= min_fee.into(),
                    "computed fee {:?} should be at least usd_min_fee {:?}",
                    fee,
                    min_fee,
                );
            });
        }
    }
}
