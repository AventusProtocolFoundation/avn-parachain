#![cfg(test)]

use crate::*;
use mock::{EthBridge, ExtBuilder, TestRuntime};
use sp_avn_common::event_types::{EthEvent, EthEventId, EventData, LiftedData, ValidEvents};
use sp_core::{H160, U256};

use sp_runtime::testing::{TestSignature, UintAuthorityId};

use self::mock::RuntimeOrigin;

fn event_data_set() -> Vec<DiscoveredEvent> {
    let events = (1..=100)
        .map(|i| DiscoveredEvent {
            event: EthEvent {
                event_id: EthEventId {
                    signature: ValidEvents::Lifted.signature(),
                    transaction_hash: H256::from([i; 32]),
                },
                event_data: EventData::LogLifted(LiftedData {
                    token_contract: H160::from([1u8; 20]),
                    sender_address: H160::from([2u8; 20]),
                    receiver_address: H256::from([3u8; 32]),
                    amount: 1000,
                    nonce: U256::from(1),
                }),
            },
            block: i as u64,
        })
        .collect();
    events
}

#[derive(Clone)]
pub struct DiscoveredEthContext {
    pub discovered_events: Vec<DiscoveredEvent>,
    pub discovered_events_fractions: Vec<DiscoveredEthEventsFraction>,
    pub author: Author<TestRuntime>,
    pub active_ethereum_range: EthBlockRange,
}

impl Default for DiscoveredEthContext {
    fn default() -> Self {
        let primary_validator_id = 1;
        let author = Author::<TestRuntime> {
            key: UintAuthorityId(primary_validator_id),
            account_id: primary_validator_id,
        };
        let events = event_data_set();

        Self {
            author,
            discovered_events_fractions: events_helpers::discovered_eth_events_partition_factory(
                events.clone(),
            ),
            discovered_events: events,
            active_ethereum_range: EthBridge::active_ethereum_range()
                .expect("Range is active")
                .range,
        }
    }
}

impl DiscoveredEthContext {
    fn submit_events_fraction(&self, index: usize) -> DispatchResultWithPostInfo {
        EthBridge::submit_ethereum_events(
            RuntimeOrigin::none(),
            self.author.clone(),
            self.active_ethereum_range.clone(),
            self.discovered_events_fractions.get(index).expect("index exists").clone(),
            self.generate_signature(index),
        )
    }

    fn generate_signature(&self, index: usize) -> TestSignature {
        self.author
            .key
            .sign(&(self.discovered_events_fractions.get(index)).encode())
            .expect("Signature is signed")
    }
}

mod submit_discovered_events {

    use super::{DiscoveredEthContext as Context, *};
    use frame_support::{assert_noop, assert_ok};
    use sp_runtime::traits::Saturating;

    fn expected_votes_for_id(id: &H256) -> usize {
        let mut votes_count = 0usize;
        DiscoveredEventsFractions::<TestRuntime>::iter().for_each(|(_range, fraction, _votes)| {
            if fraction.id() == id {
                votes_count.saturating_inc();
            }
        });
        votes_count
    }

    #[test]
    fn adds_vote_correctly() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            ActiveEthereumRange::<TestRuntime>::put(ActiveEthRange {
                range: EthBlockRange { start_block: 1, length: 1000 },
                status: EventProcessingStatus::UnderValidation,
            });
            let context: Context = Default::default();

            for (i, _range) in context.discovered_events_fractions.iter().enumerate() {
                assert_ok!(context.submit_events_fraction(i));
            }

            assert_eq!(
                expected_votes_for_id(context.discovered_events_fractions[0].id()),
                context.discovered_events_fractions.len(),
                "Should be the same."
            );
            let status = EthBridge::active_ethereum_range().expect("Should be ok").status;
            assert_eq!(status, EventProcessingStatus::UnderValidation, "Should be UnderValidation");
        });
    }

    #[test]
    fn finalises_vote() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            ActiveEthereumRange::<TestRuntime>::put(ActiveEthRange {
                range: EthBlockRange { start_block: 1, length: 1000 },
                status: EventProcessingStatus::UnderValidation,
            });

            let contexts = (1..6 as u64)
                .map(|id| Context {
                    author: Author::<TestRuntime> { key: UintAuthorityId(id), account_id: id },
                    ..Default::default()
                })
                .take(AVN::<TestRuntime>::quorum() as usize)
                .collect::<Vec<Context>>();

            for context in contexts.iter() {
                for (i, _range) in context.discovered_events_fractions.iter().enumerate() {
                    assert_ok!(context.submit_events_fraction(i));
                }
            }
            let status = EthBridge::active_ethereum_range().expect("Should be ok").status;
            assert_eq!(status, EventProcessingStatus::Accepted(4), "Should be Accepted");
        });
    }

    mod fails_when {
        use super::*;

        #[test]
        fn range_is_not_active() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let active_range = ActiveEthRange {
                    range: EthBlockRange { start_block: 1, length: 1000 },
                    status: EventProcessingStatus::UnderValidation,
                };
                ActiveEthereumRange::<TestRuntime>::put(active_range.clone());
                let context: Context = Context {
                    active_ethereum_range: active_range.range.next_range(),
                    ..Default::default()
                };

                assert_noop!(
                    context.submit_events_fraction(0),
                    Error::<TestRuntime>::NonActiveEthereumRange,
                );
            });
        }

        #[test]
        fn voting_round_has_finished() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let active_range = ActiveEthRange {
                    range: EthBlockRange { start_block: 1, length: 1000 },
                    status: EventProcessingStatus::Accepted(1),
                };
                ActiveEthereumRange::<TestRuntime>::put(active_range.clone());
                let context: Context = Default::default();

                assert_noop!(context.submit_events_fraction(0), Error::<TestRuntime>::VotingEnded,);
            });
        }

        #[test]
        fn author_has_vote_the_fraction() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let active_range = ActiveEthRange {
                    range: EthBlockRange { start_block: 1, length: 1000 },
                    status: EventProcessingStatus::UnderValidation,
                };
                ActiveEthereumRange::<TestRuntime>::put(active_range.clone());
                let context: Context = Default::default();

                assert_ok!(context.submit_events_fraction(0));

                // Resubmit vote
                assert_noop!(
                    context.submit_events_fraction(0),
                    Error::<TestRuntime>::EventVoteExists,
                );
            });
        }
    }
}
