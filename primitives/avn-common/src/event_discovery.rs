use crate::*;

use codec::{Decode, Encode, MaxEncodedLen};
use event_types::EthEvent;
use sp_core::{bounded::BoundedBTreeSet, ConstU32};
use sp_runtime::traits::Saturating;

use self::event_types::ValidEvents;

pub type VotesLimit = ConstU32<100>;
pub type EventsBatchLimit = ConstU32<32>;

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
        let end_block = self.start_block.saturating_add(self.length).saturating_less_one();
        (self.start_block, end_block)
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct DiscoveredEvent {
    pub event: EthEvent,
    pub block: u64,
}

impl PartialOrd for DiscoveredEvent {
    fn partial_cmp(&self, other: &Self) -> Option<scale_info::prelude::cmp::Ordering> {
        // TODO ensure that the comparison is lowercase.
        let ord_sig = self.event.event_id.signature.partial_cmp(&other.event.event_id.signature);

        if let Some(core::cmp::Ordering::Equal) = ord_sig {
            return ord_sig
        }

        match self.block.partial_cmp(&other.block) {
            Some(core::cmp::Ordering::Equal) => {},
            ord => return ord,
        }
        ord_sig
    }
}

impl Ord for DiscoveredEvent {
    fn cmp(&self, other: &Self) -> scale_info::prelude::cmp::Ordering {
        // TODO ensure that the comparison is lowercase.
        let ord_sig = self.event.event_id.signature.cmp(&other.event.event_id.signature);

        if let core::cmp::Ordering::Equal = ord_sig {
            return ord_sig
        }

        match self.block.cmp(&other.block) {
            core::cmp::Ordering::Equal => {},
            ord => return ord,
        }
        ord_sig
    }
}

type EthereumEventsSet = BoundedBTreeSet<DiscoveredEvent, EventsBatchLimit>;
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
        use sp_io::hashing::blake2_256;
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
    fn get_filter() -> EthBridgeEventsFilter;
}

impl EthereumEventsFilterTrait for () {
    fn get_filter() -> EthBridgeEventsFilter {
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

pub mod events_helpers {
    use super::*;
    pub extern crate alloc;
    use alloc::collections::BTreeSet;

    pub fn discovered_eth_events_partition_factory(
        range: EthBlockRange,
        events: Vec<DiscoveredEvent>,
    ) -> Vec<EthereumEventsPartition> {
        let sorted_events = {
            let mut mut_events = events.clone();
            mut_events.sort();
            mut_events
        };

        let chunk_size: usize = <EventsBatchLimit as sp_core::Get<u32>>::get() as usize;
        let mut partitions = Vec::<EthereumEventsPartition>::new();

        let event_chunks: Vec<_> = sorted_events.chunks(chunk_size).collect();
        let partitions_count = event_chunks.len();

        let _ =
            event_chunks
                .iter()
                .enumerate()
                .try_for_each(|(partition, chunk)| -> Result<(), ()> {
                    let inner_data: BTreeSet<DiscoveredEvent> = chunk.iter().cloned().collect();
                    let data = EthereumEventsSet::try_from(inner_data)?;
                    partitions.push(EthereumEventsPartition::new(
                        range.clone(),
                        partition as u16,
                        partitions_count == partition.saturating_add(1),
                        data,
                    ));
                    Ok(())
                });
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

    // TODO unit test this
    pub fn compute_finalised_block_range_for_latest_ethereum_block(
        ethereum_block: u32,
    ) -> EthBlockRange {
        let length = 20u32;
        let calculation_block = ethereum_block.saturating_sub(5 * length);
        let start_block = calculation_block - calculation_block % length;

        EthBlockRange { start_block, length }
    }
}
