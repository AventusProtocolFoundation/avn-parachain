// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Autogenerated weights for pallet_avn_transaction_payment
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2023-05-29, STEPS: `50`, REPEAT: 20, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! HOSTNAME: `nahu-XPS-8930`, CPU: `Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("dev"), DB CACHE: 1024

// Executed Command:
// ./target/production/avn-parachain-collator
// benchmark
// pallet
// --chain
// dev
// --execution=wasm
// --wasm-execution=compiled
// --template
// .maintain/frame-weight-template.hbs
// --pallet
// pallet_avn-transaction_payment
// --extrinsic
// *
// --steps
// 50
// --repeat
// 20
// --output
// ../avn-parachain/pallets/avn-transaction-payment/src/default_weights.rs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_avn_transaction_payment.
pub trait WeightInfo {
	fn set_known_sender() -> Weight;
	fn remove_known_sender() -> Weight;
}

/// Weights for pallet_avn_transaction_payment using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// Storage: AvnTransactionPayment KnownSenders (r:1 w:1)
	fn set_known_sender() -> Weight {
		Weight::from_ref_time(17_910_000)
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	// Storage: AvnTransactionPayment KnownSenders (r:1 w:1)
	fn remove_known_sender() -> Weight {
		Weight::from_ref_time(19_447_000)
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	// Storage: AvnTransactionPayment KnownSenders (r:1 w:1)
	fn set_known_sender() -> Weight {
		Weight::from_ref_time(17_910_000)
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
	// Storage: AvnTransactionPayment KnownSenders (r:1 w:1)
	fn remove_known_sender() -> Weight {
		Weight::from_ref_time(19_447_000)
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
}