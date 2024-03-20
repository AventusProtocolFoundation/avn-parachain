use crate::*;

use codec::{Decode, Encode, MaxEncodedLen};
use event_types::EthEvent;
use sp_core::{bounded::BoundedBTreeSet, ConstU32};
use sp_io::hashing::blake2_256;
use sp_runtime::traits::Saturating;

pub type VotesLimit = ConstU32<100>;
pub type EventsBatchLimit = ConstU32<32>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
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

pub type FractionsCount = u16;
type EthEventsPartition = BoundedBTreeSet<DiscoveredEvent, EventsBatchLimit>;
#[derive(PartialEq, Eq, Clone, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
pub struct DiscoveredEthEventsFraction {
    id: H256,
    fraction: FractionsCount,
    fraction_count: FractionsCount,
    data: EthEventsPartition,
}

impl DiscoveredEthEventsFraction {
    pub fn id(&self) -> &H256 {
        &self.id
    }

    pub fn fraction(&self) -> FractionsCount {
        self.fraction
    }

    pub fn fraction_count(&self) -> FractionsCount {
        self.fraction
    }

    pub fn events(&self) -> &EthEventsPartition {
        &self.data
    }

    pub fn is_valid(&self) -> bool {
        self.fraction < self.fraction_count
    }

    fn new(
        data: EthEventsPartition,
        fraction: FractionsCount,
        fraction_count: FractionsCount,
        id: &H256,
    ) -> Self {
        DiscoveredEthEventsFraction { data, fraction, fraction_count, id: id.clone() }
    }
}

pub mod events_helpers {
    use super::*;
    pub extern crate alloc;
    use alloc::collections::BTreeSet;

    pub fn discovered_eth_events_partition_factory(
        events: Vec<DiscoveredEvent>,
    ) -> Vec<DiscoveredEthEventsFraction> {
        let mut sorted = events.clone();
        sorted.sort();
        let chunk_size: usize = <EventsBatchLimit as sp_core::Get<u32>>::get() as usize;
        let mut fractions = Vec::<DiscoveredEthEventsFraction>::new();

        let mut iter = sorted.chunks(chunk_size).enumerate();
        let fraction_count = sorted.chunks(chunk_size).count() as FractionsCount;
        let hash: H256 = blake2_256(&(&events, fraction_count).encode()).into();

        let _ = iter.try_for_each(|(fraction, chunk)| -> Result<(), ()> {
            let inner_data: BTreeSet<DiscoveredEvent> = chunk.iter().cloned().collect();
            let data = EthEventsPartition::try_from(inner_data)?;
            fractions.push(DiscoveredEthEventsFraction::new(
                data,
                fraction as FractionsCount,
                fraction_count,
                &hash,
            ));
            Ok(())
        });
        fractions
    }
}
