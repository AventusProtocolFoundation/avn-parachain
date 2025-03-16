// Copyright 2023 Aventus Network Systems (UK) Ltd.

#![cfg(test)]
use crate::{mock::*, *};
use frame_support::assert_ok;
use sp_runtime::DispatchError;
pub extern crate alloc;
use alloc::collections::BTreeSet;
use sp_avn_common::{event_discovery::EthBridgeEventsFilter, event_types::ValidEvents};

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
        additional_events: Default::default(),
    });
}

mod process_events {
    use super::*;

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
                mock::RuntimeEvent::EthBridge(Event::<TestRuntime>::EventAccepted {
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
                mock::RuntimeEvent::EthBridge(Event::<TestRuntime>::EventRejected {
                    eth_event_id: context.bad_eth_event_id.clone(),
                    reason: DispatchError::Other("").into(),
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
                mock::RuntimeEvent::EthBridge(Event::<TestRuntime>::EventRejected {
                    eth_event_id: context.eth_event_id.clone(),
                    reason: Error::<TestRuntime>::EventAlreadyProcessed.into(),
                })));
        });
    }
}
