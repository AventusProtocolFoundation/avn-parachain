// Copyright 2023 Aventus Network Systems (UK) Ltd.

#![cfg(test)]
use crate::{
    eth::{generate_send_calldata, *},
    mock::*,
    tx::*,
    *,
};
const ROOT_HASH: &str = "30b83f0d722d1d4308ab4660a72dbaf0a7392d5674eca3cd21d57256d42df7a0";
const REWARDS: &[u8] = b"15043665996000000000";
const AVG_STAKED: &[u8] = b"9034532443555111110000";
const PERIOD: &[u8] = b"3";
const T2_PUB_KEY: &str = "14aeac90dbd3573458f9e029eb2de122ee94f2f0bc5ee4b6c6c5839894f1a547";
const T1_PUB_KEY: &str = "23d79f6492dddecb436333a5e7a4cfcc969f568e01283fa2964aae15327fb8a3b685a4d0f3ef9b3c2adb20f681dbc74b7f82c1cf8438d37f2c10e9c79591e9ea";

#[test]
fn check_publish_root_encoding() {
    let function_name = b"publishRoot".to_vec();
    let params = vec![(b"bytes32".to_vec(), hex::decode(ROOT_HASH).unwrap())];
    let expected_msg_hash = "778a3de5c54e9f2d1c0249cc5c15edf56e205daca24349cc6a71ee0ab04b6300";
    let expected_calldata = "0664c0ba30b83f0d722d1d4308ab4660a72dbaf0a7392d5674eca3cd21d57256d42df7a000000000000000000000000000000000000000000000000000000000651407c9000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000";

    run_checks(function_name, params, expected_msg_hash, expected_calldata);
}

#[test]
fn check_trigger_growth_encoding() {
    let function_name = b"triggerGrowth".to_vec();
    let params = vec![
        (b"uint128".to_vec(), REWARDS.to_vec()),
        (b"uint128".to_vec(), AVG_STAKED.to_vec()),
        (b"uint32".to_vec(), PERIOD.to_vec()),
    ];
    let expected_msg_hash = "1b45b1eed67d67a0bb55ea988e7a386fc0cfe2e6a7b391485dec22cbd08e5d67";
    let expected_calldata = "59ef631d000000000000000000000000000000000000000000000000d0c5d684c333f8000000000000000000000000000000000000000000000001e9c352fe68b4400570000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000651407c9000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000";

    run_checks(function_name, params, expected_msg_hash, expected_calldata);
}

#[test]
fn check_add_author_encoding() {
    let function_name = b"addAuthor".to_vec();
    let params = vec![
        (b"bytes".to_vec(), hex::decode(T1_PUB_KEY).unwrap()),
        (b"bytes32".to_vec(), hex::decode(T2_PUB_KEY).unwrap()),
    ];
    let expected_msg_hash = "bad82d9066614ce5ee4c86a8858c6adebbff57f81200ca2ad0a7f400ff388ad4";
    let expected_calldata = "b685115200000000000000000000000000000000000000000000000000000000000000a014aeac90dbd3573458f9e029eb2de122ee94f2f0bc5ee4b6c6c5839894f1a54700000000000000000000000000000000000000000000000000000000651407c900000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000004023d79f6492dddecb436333a5e7a4cfcc969f568e01283fa2964aae15327fb8a3b685a4d0f3ef9b3c2adb20f681dbc74b7f82c1cf8438d37f2c10e9c79591e9ea0000000000000000000000000000000000000000000000000000000000000000";

    run_checks(function_name, params, expected_msg_hash, expected_calldata);
}

#[test]
fn check_remove_author_encoding() {
    let function_name = b"removeAuthor".to_vec();
    let params = vec![
        (b"bytes32".to_vec(), hex::decode(T2_PUB_KEY).unwrap()),
        (b"bytes".to_vec(), hex::decode(T1_PUB_KEY).unwrap()),
    ];
    let expected_msg_hash = "01d244c875c7f80c472dde109dc8d80d43e4f513f7349484b37ba8b586ea5b81";
    let expected_calldata = "146b3b5214aeac90dbd3573458f9e029eb2de122ee94f2f0bc5ee4b6c6c5839894f1a54700000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000651407c900000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000004023d79f6492dddecb436333a5e7a4cfcc969f568e01283fa2964aae15327fb8a3b685a4d0f3ef9b3c2adb20f681dbc74b7f82c1cf8438d37f2c10e9c79591e9ea0000000000000000000000000000000000000000000000000000000000000000";

    run_checks(function_name, params, expected_msg_hash, expected_calldata);
}

fn run_checks(
    function_name: Vec<u8>,
    params: Vec<(Vec<u8>, Vec<u8>)>,
    expected_msg_hash: &str,
    expected_calldata: &str,
) {
    let mut ext = ExtBuilder::build_default()
        .with_validators()
        .with_genesis_config()
        .as_externality();
    ext.execute_with(|| {
        let current_time = 1_695_809_729_000;
        pallet_timestamp::Pallet::<TestRuntime>::set_timestamp(current_time);

        let tx_id = add_new_request::<TestRuntime>(&function_name, &params).unwrap();
        let active_tx = ActiveTransaction::<TestRuntime>::get().expect("is active");
        assert_eq!(tx_id, active_tx.id);

        let eth_tx_lifetime_secs = EthBridge::get_eth_tx_lifetime_secs();
        let expected_expiry = current_time / 1000 + eth_tx_lifetime_secs;
        assert_eq!(active_tx.expiry, expected_expiry);

        let msg_hash = hex::encode(active_tx.msg_hash);
        assert_eq!(msg_hash, expected_msg_hash);

        let calldata = generate_send_calldata::<TestRuntime>(&active_tx).unwrap();
        let calldata = hex::encode(calldata);
        assert_eq!(calldata, expected_calldata);
    })
}

#[cfg(test)]
mod add_confirmation {

    use super::*;
    use frame_support::assert_ok;
    use frame_system::RawOrigin;

    #[test]
    fn it_adds_confirmation_correctly() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = setup_context();

            let tx_id = add_new_request::<TestRuntime>(
                &context.request_function_name,
                &context.request_params,
            )
            .unwrap();

            let active_tx = ActiveTransaction::<TestRuntime>::get()
                .expect("Active transaction should be present");

            assert_ok!(EthBridge::add_confirmation(
                RawOrigin::None.into(),
                tx_id,
                context.confirmation_signature.clone(),
                context.confirming_author.clone(),
                context.test_signature
            ));

            let active_tx = ActiveTransaction::<TestRuntime>::get()
                .expect("Active transaction should be present");

            assert!(
                active_tx.confirmations.contains(&context.confirmation_signature),
                "Confirmation should be present"
            );

            assert_eq!(
                active_tx.data.sender, context.author.account_id,
                "Sender should be the author's account_id"
            );
        });
    }

    // #[test]
    // fn it_returns_error_when_transaction_is_not_active() {
    //     let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    //     ext.execute_with(|| {
    //         let context = setup_context();
    //         let tx_id = 999; // Non-existent transaction id

    //         let result = EthBridge::add_confirmation(
    //             RawOrigin::None.into(),
    //             tx_id,
    //             context.confirmation_signature,
    //             context.author,
    //             context.test_signature.clone()
    //         );

    //         assert_eq!(result, Err(Error::<TestRuntime>::Se.into()));
    //     });
    // }

    #[test]
    fn it_returns_error_when_same_confirmation_added_twice() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = setup_context();

            let tx_id = add_new_request::<TestRuntime>(
                &context.request_function_name,
                &context.request_params,
            )
            .unwrap();

            // Add the same confirmation twice
            let _ = EthBridge::add_confirmation(
                RawOrigin::None.into(),
                tx_id,
                context.confirmation_signature.clone(),
                context.confirming_author.clone(),
                context.test_signature.clone()
            );

            let result = EthBridge::add_confirmation(
                RawOrigin::None.into(),
                tx_id,
                context.confirmation_signature.clone(),
                context.confirming_author.clone(),
                context.test_signature.clone()
            );

            assert_eq!(result, Err(Error::<TestRuntime>::DuplicateConfirmation.into()));
        });
    }
}

#[cfg(test)]
mod add_eth_tx_hash {
    use super::*;
    use frame_support::assert_ok;
    use frame_system::RawOrigin;

    fn run_eth_tx_hash_test(setup_fn: Option<fn(&mut ActiveTransactionData<TestRuntime>)>, expected: DispatchResultWithPostInfo) {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = setup_context();

            let tx_id = add_new_request::<TestRuntime>(
                &context.request_function_name,
                &context.request_params,
            )
            .unwrap();

            if let Some(setup_fn) = setup_fn {
                let mut active_tx = ActiveTransaction::<TestRuntime>::get().expect("is active");
                setup_fn(&mut active_tx);
                ActiveTransaction::<TestRuntime>::put(active_tx);
            }

            let result = EthBridge::add_eth_tx_hash(
                RawOrigin::None.into(),
                tx_id,
                context.eth_tx_hash,
                context.author,
                context.test_signature.clone()
            );

            assert_eq!(result, expected);
        });
    }

    #[test]
    fn it_returns_error_when_eth_tx_hash_already_set() {
        run_eth_tx_hash_test(Some(|active_tx| {
            active_tx.data.eth_tx_hash = H256::repeat_byte(1);
        }), Err(Error::<TestRuntime>::EthTxHashAlreadySet.into()));
    }

    #[test]
    fn it_returns_error_when_eth_tx_hash_must_be_set_by_sender() {
        run_eth_tx_hash_test(Some(|active_tx| {
            active_tx.data.sender = Default::default();
        }), Err(Error::<TestRuntime>::EthTxHashMustBeSetBySender.into()));
    }

    #[test]
    fn it_sets_eth_tx_hash_correctly() {
        run_eth_tx_hash_test(None, Ok(().into()));
    }
}

#[cfg(test)]
mod add_corroboration {
    use super::*;
    use frame_support::assert_ok;
    use frame_system::RawOrigin;
    fn run_corroboration_test(
        is_tx_successful: bool,
        is_hash_valid: bool,
        assertion: fn(ActiveTransactionData<TestRuntime>),
    ) {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = setup_context();

            let tx_id = add_new_request::<TestRuntime>(
                &context.request_function_name,
                &context.request_params,
            )
            .unwrap();

            assert_ok!(EthBridge::add_corroboration(
                RawOrigin::None.into(),
                tx_id,
                is_tx_successful,
                is_hash_valid,
                context.confirming_author.clone(),
                context.test_signature
            ));

            let active_tx = ActiveTransaction::<TestRuntime>::get()
                .expect("Active transaction should be present");

            assertion(active_tx);
        });
    }



    #[test]
    fn it_adds_invalid_hash_and_successful_corroboration_correctly() {
        run_corroboration_test(true, true, |active_tx| {
            assert!(active_tx.valid_tx_hash_corroborations.len() > 0);
        });
    }

    #[test]
    fn it_adds_invalid_hash_and_failure_corroboration_correctly() {
        run_corroboration_test(true, false, |active_tx| {
            assert!(active_tx.invalid_tx_hash_corroborations.len() > 0);
        });
    }

    #[test]
    fn it_adds_successfull_corroboration_correctly() {
        run_corroboration_test(true, true, |active_tx| {
            assert!(active_tx.valid_tx_hash_corroborations.len() > 0);
        });
    }
}
