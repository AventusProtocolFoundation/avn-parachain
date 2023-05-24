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
    assert_ok
};

fn to_acc_id(id: u64) -> AccountId {
    return TestAccount::new(id).account_id()
}

#[test]
fn set_known_senders(){
 new_test_ext().execute_with(|| {

    // let context: Context = Default::default();
    let account_1 = to_acc_id(1u64);

    let config = AdjustmentInput::<TestRuntime> {
        fee_type: FeeType::PercentageFee(PercentageFeeConfig {
            percentage: 10,
            _marker: sp_std::marker::PhantomData::<TestRuntime>
        }),
        adjustment_type: None,
    };

    // assert_eq!(true, true);
    assert_ok!(AvnTransactionPayment::set_known_sender(
        RuntimeOrigin::root(),
        account_1,
        config,
    ));



//    assert_err!(
//      AvnTransactionPayment::set_known_sender(RuntimeOrigin::signed(1), 51),
//      "value must be <= maximum add amount constant"
//    );
 })
}