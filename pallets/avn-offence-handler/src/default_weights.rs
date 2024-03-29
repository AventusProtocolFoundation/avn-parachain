
//! Autogenerated weights for pallet_avn_offence_handler
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2024-02-15, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `ip-172-31-16-102`, CPU: `Intel(R) Xeon(R) Platinum 8275CL CPU @ 3.00GHz`
//! EXECUTION: ``, WASM-EXECUTION: `Compiled`, CHAIN: `Some("dev")`, DB CACHE: `1024`

// Executed Command:
// ./avn-parachain-collator
// benchmark
// pallet
// --chain
// dev
// --wasm-execution=compiled
// --template
// frame-weight-template.hbs
// --pallet
// pallet_avn_offence_handler
// --extrinsic
// *
// --steps
// 50
// --repeat
// 20
// --output
// avt_offence_handlerweights.rs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for pallet_avn_offence_handler.
pub trait WeightInfo {
	fn configure_slashing() -> Weight;
}

/// Weights for pallet_avn_offence_handler using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `AvnOffenceHandler::SlashingEnabled` (r:0 w:1)
	/// Proof: `AvnOffenceHandler::SlashingEnabled` (`max_values`: Some(1), `max_size`: Some(1), added: 496, mode: `MaxEncodedLen`)
	fn configure_slashing() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 8_167_000 picoseconds.
		Weight::from_parts(8_564_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	/// Storage: `AvnOffenceHandler::SlashingEnabled` (r:0 w:1)
	/// Proof: `AvnOffenceHandler::SlashingEnabled` (`max_values`: Some(1), `max_size`: Some(1), added: 496, mode: `MaxEncodedLen`)
	fn configure_slashing() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 8_167_000 picoseconds.
		Weight::from_parts(8_564_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}