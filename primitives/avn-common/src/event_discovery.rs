use core::cmp::Ordering;

use crate::{
    event_types::EthTransactionId,
    *,
};

use codec::{Decode, Encode, MaxEncodedLen};
use event_types::EthEvent;
use sp_core::{bounded::BoundedBTreeSet, ConstU32};
use sp_runtime::traits::Saturating;

use self::event_types::ValidEvents;

pub const MAX_INCOMING_EVENTS_BATCH_SIZE: u32 = 32u32;
pub type IncomingEventsBatchLimit = ConstU32<MAX_INCOMING_EVENTS_BATCH_SIZE>;

#[derive(
    Encode, Decode, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Default, TypeInfo, MaxEncodedLen,
)]
pub struct EthBlockRange {
    pub start_block: u32,
    pub length: u32,
}

impl EthBlockRange {
    pub fn next_range(&self) -> EthBlockRange {
        EthBlockRange {
            start_block: self.start_block.saturating_add(self.length),
            length: self.length,
        }
    }
    pub fn range(&self) -> (u32, u32) {
        (self.start_block, self.end_block())
    }
    pub fn end_block(&self) -> u32 {
        self.start_block.saturating_add(self.length).saturating_less_one()
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct DiscoveredEvent {
    pub event: EthEvent,
    pub block: u64,
}

impl PartialOrd for DiscoveredEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DiscoveredEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.block.cmp(&other.block) {
            Ordering::Equal =>
                self.event.event_id.transaction_hash.cmp(&other.event.event_id.transaction_hash),
            ord => ord,
        }
    }
}

type EthereumEventsSet = BoundedBTreeSet<DiscoveredEvent, IncomingEventsBatchLimit>;
#[derive(PartialEq, Eq, Clone, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
pub struct EthereumEventsPartition {
    range: EthBlockRange,
    partition: u16,
    is_last: bool,
    data: EthereumEventsSet,
}

impl EthereumEventsPartition {
    pub fn partition(&self) -> u16 {
        self.partition
    }

    pub fn events(&self) -> &EthereumEventsSet {
        &self.data
    }

    pub fn range(&self) -> &EthBlockRange {
        &self.range
    }

    pub fn is_last(&self) -> bool {
        self.is_last
    }

    pub fn id(&self) -> H256 {
        blake2_256(&(&self).encode()).into()
    }

    pub fn new(
        range: EthBlockRange,
        partition: u16,
        is_last: bool,
        data: EthereumEventsSet,
    ) -> Self {
        EthereumEventsPartition { range, partition, is_last, data }
    }
}

pub type EventsTypesLimit = ConstU32<20>;
pub type EthBridgeEventsFilter = BoundedBTreeSet<ValidEvents, EventsTypesLimit>;

pub trait EthereumEventsFilterTrait {
    /// Returns all events included in the filter.
    fn get() -> EthBridgeEventsFilter;

    /// Returns only primary events included in the filter.
    fn get_primary() -> EthBridgeEventsFilter {
        let mut events_filter = Self::get();
        for event in ValidEvents::values().iter().filter(|e| !e.is_primary()) {
            events_filter.remove(event);
        }
        events_filter
    }
}

impl EthereumEventsFilterTrait for () {
    fn get() -> EthBridgeEventsFilter {
        Default::default()
    }
}

pub fn encode_eth_event_submission_data<AccountId: Encode, Data: Encode>(
    context: &[u8],
    account_id: &AccountId,
    data: Data,
) -> Vec<u8> {
    log::debug!(
        "🪲 Encoding submission data: [ context {:?} - account {:?} - data {:?} ]",
        context,
        account_id.encode(),
        &data.encode()
    );
    (context, &account_id, data).encode()
}

pub type AdditionalEvents = BoundedBTreeSet<EthTransactionId, ConstU32<16>>;

pub mod events_helpers {
    use super::*;
    pub extern crate alloc;
    use alloc::collections::BTreeSet;

    pub struct EthereumEventsPartitionFactory {}

    impl EthereumEventsPartitionFactory {
        pub fn create_partitions(
            range: EthBlockRange,
            events: Vec<DiscoveredEvent>,
        ) -> Vec<EthereumEventsPartition> {
            let sorted_events = {
                let mut mut_events = events.clone();
                mut_events.sort();
                mut_events
            };

            let chunk_size: usize = <IncomingEventsBatchLimit as sp_core::Get<u32>>::get() as usize;
            let mut partitions = Vec::<EthereumEventsPartition>::new();

            let event_chunks: Vec<_> = sorted_events.chunks(chunk_size).collect();
            let partitions_count = event_chunks.len();

            let _ = event_chunks.iter().enumerate().try_for_each(
                |(partition, chunk)| -> Result<(), ()> {
                    let inner_data: BTreeSet<DiscoveredEvent> = chunk.iter().cloned().collect();
                    let data = EthereumEventsSet::try_from(inner_data)?;
                    partitions.push(EthereumEventsPartition::new(
                        range.clone(),
                        partition as u16,
                        partitions_count == partition.saturating_add(1),
                        data,
                    ));
                    Ok(())
                },
            );
            if partitions.is_empty() {
                partitions.push(EthereumEventsPartition::new(
                    range.clone(),
                    0,
                    true,
                    Default::default(),
                ))
            }
            partitions
        }
    }

    // TODO unit test this
    pub fn compute_start_block_from_finalised_block_number(
        ethereum_block: u32,
        range_length: u32,
    ) -> Result<u32, ()> {
        let calculation_block = ethereum_block.saturating_sub(5 * range_length);
        let rem = calculation_block.checked_rem(range_length).ok_or(())?;
        Ok(calculation_block.saturating_sub(rem))
    }
}
