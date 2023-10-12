//! # Ethereum events pallet
// Copyright 2022 Aventus Systems (UK) Ltd.

//! ethereum-events pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::{EventRecord, RawOrigin};

benchmarks! {
    set_bridge_contract {
        let contract_address = H160::from([1; 20]);
    }: set_bridge_contract(RawOrigin::Root, contract_address.clone())
    verify {
        assert!(<AvnBridgeContractAddress<T>>::get() == contract_address);
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().as_externality(),
    crate::mock::TestRuntime,
);
