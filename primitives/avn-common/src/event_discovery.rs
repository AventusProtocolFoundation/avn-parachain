use crate::*;

use codec::{Decode, Encode, MaxEncodedLen};
use event_types::EthEvent;
use sp_core::{bounded::BoundedBTreeSet, ConstU32};
use sp_io::hashing::blake2_256;

pub type VotesLimit = ConstU32<100>;
pub type EventsBatchLimit = ConstU32<32>;

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct EthBlockRange {
    pub start_block: u32,
    pub length: u32,
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

type EthEventsPartition = BoundedBTreeSet<DiscoveredEvent, EventsBatchLimit>;
#[derive(PartialEq, Eq, Clone, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
pub struct DiscoveredEthEventsFraction {
    data: EthEventsPartition,
    fraction: u16,
    fraction_count: u16,
    id: H256,
}

impl DiscoveredEthEventsFraction {
    pub fn events(&self) -> &EthEventsPartition {
        &self.data
    }

    pub fn fraction(&self) -> u16 {
        self.fraction
    }

    pub fn fraction_count(&self) -> u16 {
        self.fraction
    }

    pub fn id(&self) -> &H256 {
        &self.id
    }

    fn new(data: EthEventsPartition, fraction: u16, fraction_count: u16, id: &H256) -> Self {
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
        let fraction_count = sorted.chunks(chunk_size).count() as u16;
        let hash: H256 = blake2_256(&(&events, fraction_count).encode()).into();

        let _ = iter.try_for_each(|(fraction, chunk)| -> Result<(), ()> {
            let inner_data: BTreeSet<DiscoveredEvent> = chunk.iter().cloned().collect();
            let data = EthEventsPartition::try_from(inner_data)?;
            fractions.push(DiscoveredEthEventsFraction::new(
                data,
                fraction as u16,
                fraction_count,
                &hash,
            ));
            Ok(())
        });
        fractions
    }
}
