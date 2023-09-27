#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_avn
pub trait WeightInfo {
	fn set_eth_tx_lifetime_secs() -> Weight;
}

/// Weights for pallet_avn using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn set_eth_tx_lifetime_secs() -> Weight {
        (Weight::from_ref_time(5_140_000))
        .saturating_add(T::DbWeight::get().writes(1))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn set_eth_tx_lifetime_secs() -> Weight {
		(Weight::from_ref_time(5_140_000))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
}