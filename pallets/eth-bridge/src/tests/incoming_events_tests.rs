// Copyright 2023 Aventus Network Systems (UK) Ltd.

#![cfg(test)]
use crate::{eth::generate_send_calldata, mock::*, request::*, *};
use frame_support::{
    assert_err, assert_noop, assert_ok, dispatch::DispatchResultWithPostInfo, error::BadOrigin,
};
use sp_runtime::{testing::UintAuthorityId, DispatchError};
pub extern crate alloc;
use alloc::collections::BTreeSet;
use sp_avn_common::event_discovery::EthBridgeEventsFilter;

const ROOT_HASH: &str = "30b83f0d722d1d4308ab4660a72dbaf0a7392d5674eca3cd21d57256d42df7a0";
const REWARDS: &[u8] = b"15043665996000000000";
const AVG_STAKED: &[u8] = b"9034532443555111110000";
const PERIOD: &[u8] = b"3";
const T2_PUB_KEY: &str = "14aeac90dbd3573458f9e029eb2de122ee94f2f0bc5ee4b6c6c5839894f1a547";
const T1_PUB_KEY: &str = "23d79f6492dddecb436333a5e7a4cfcc969f568e01283fa2964aae15327fb8a3b685a4d0f3ef9b3c2adb20f681dbc74b7f82c1cf8438d37f2c10e9c79591e9ea";

// Added this function as in event_listener_tests to initialize the active event range
fn init_active_range() {
    ActiveEthereumRange::<TestRuntime>::put(ActiveEthRange {
        range: EthBlockRange { start_block: 1, length: 1000 },
        partition: 0,
        event_types_filter: EthBridgeEventsFilter::try_from(
            vec![
                ValidEvents::AddedValidator,
                ValidEvents::Lifted,
                ValidEvents::AvtGrowthLifted,
                ValidEvents::AvtLowerClaimed,
            ]
            .into_iter()
            .collect::<BTreeSet<ValidEvents>>(),
        )
        .unwrap(),
    });
}

mod process_events {
    use super::*;
    use sp_avn_common::event_types::EthEventId;

    // succesfully process the specified ethereum_event
    #[test]
    fn succesful_event_processing_accepted() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = setup_context();
            init_active_range();

            // Two calls needed as upon the first there are not enough votes to pass the condition
            // in lib.rs line 563, to reach the call of process_ethereum_events_partition()
            assert_ok!(EthBridge::submit_ethereum_events(
                RuntimeOrigin::none(),
                context.author.clone(),
                context.mock_event_partition.clone(),
                context.test_signature.clone()
            ));
            assert_ok!(EthBridge::submit_ethereum_events(
                RuntimeOrigin::none(),
                context.author_two.clone(),
                context.mock_event_partition.clone(),
                context.test_signature_two.clone()
            ));

            assert!(System::events().iter().any(|record| record.event ==
                mock::RuntimeEvent::EthBridge(Event::<TestRuntime>::EventProcessingAccepted {
                    accepted: true,
                    eth_event_id: context.eth_event_id.clone(),
                })));
        });
    }

    // This test should fail processing the ethereum_event and emit the specified event
    #[test]
    fn succesful_event_processing_not_accepted() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = setup_context();
            init_active_range();
            assert_ok!(EthBridge::submit_ethereum_events(
                RuntimeOrigin::none(),
                context.author.clone(),
                context.bad_mock_event_partition.clone(),
                context.test_signature.clone()
            ));
            assert_ok!(EthBridge::submit_ethereum_events(
                RuntimeOrigin::none(),
                context.author_two.clone(),
                context.bad_mock_event_partition.clone(),
                context.test_signature.clone()
            ));

            assert!(System::events().iter().any(|record| record.event ==
                mock::RuntimeEvent::EthBridge(Event::<TestRuntime>::EventProcessingRejected {
                    accepted: false,
                    eth_event_id: context.bad_eth_event_id.clone(),
                })));
        });
    }

    // This test should fail on the check
    // T::ProcessedEventsChecker::processed_event_exists(&event.event_id.clone()), if the event is
    // already in the system
    #[test]
    fn event_already_processed() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = setup_context();
            init_active_range();
            assert_ok!(EthBridge::submit_ethereum_events(
                RuntimeOrigin::none(),
                context.author.clone(),
                context.mock_event_partition.clone(),
                context.test_signature.clone()
            ));
            assert_ok!(EthBridge::submit_ethereum_events(
                RuntimeOrigin::none(),
                context.author_two.clone(),
                context.mock_event_partition.clone(),
                context.test_signature_two.clone()
            ));
            assert_ok!(EthBridge::submit_ethereum_events(
                RuntimeOrigin::none(),
                context.author.clone(),
                context.second_mock_event_partition.clone(),
                context.test_signature.clone()
            ));
            assert_ok!(EthBridge::submit_ethereum_events(
                RuntimeOrigin::none(),
                context.author_two.clone(),
                context.second_mock_event_partition.clone(),
                context.test_signature_two.clone()
            ));
            assert!(System::events().iter().any(|record| record.event ==
                mock::RuntimeEvent::EthBridge(
                    Event::<TestRuntime>::DuplicateEventSubmission {
                        eth_event_id: context.eth_event_id.clone(),
                    }
                )));
        });
    }
}
