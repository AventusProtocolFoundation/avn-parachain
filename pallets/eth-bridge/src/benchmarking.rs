//! # Eth bridge pallet
// Copyright 2023 Aventus Network Services (UK) Ltd.

//! eth-bridge pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use crate::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::{ensure, pallet_prelude::ConstU32, BoundedVec};
use frame_system::RawOrigin;

pub type FunctionLimit = ConstU32<{ crate::FUNCTION_NAME_CHAR_LIMIT }>;
pub type ParamsLimit = ConstU32<{ crate::PARAMS_LIMIT }>;
pub type TypeLimit = ConstU32<{ crate::TYPE_CHAR_LIMIT }>;
pub type ValueLimit = ConstU32<{ crate::VALUE_CHAR_LIMIT }>;

fn setup_tx_data<T: Config>(tx_id: u32, num_confirmations: u32) {
    let function_name: Vec<u8> = b"publishRoot".to_vec();
    let function_name_bounded: BoundedVec<u8, FunctionLimit> =
        BoundedVec::try_from(function_name).unwrap();
    let param_type: Vec<u8> = b"bytes32".to_vec();
    let param_type_bounded: BoundedVec<u8, TypeLimit> = BoundedVec::try_from(param_type).unwrap();
    let param_value: Vec<u8> = b"bytes32".to_vec();
    let param_value_bounded: BoundedVec<u8, ValueLimit> =
        BoundedVec::try_from(param_value).unwrap();
    let params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit> =
        BoundedVec::try_from(vec![(param_type_bounded, param_value_bounded)]).unwrap();

    let tx_data = TransactionData {
        function_name: function_name_bounded,
        params,
        expiry: 1438269973u64,
        msg_hash: H256::repeat_byte(1),
        confirmations: {
            let mut confirmations = BoundedVec::default();
            for i in 0..num_confirmations {
                let confirmation: [u8; 65] = [i as u8; 65];
                confirmations.try_push(confirmation).unwrap();
            }
            confirmations
        },
        sending_author: Some([2u8; 32]),
        eth_tx_hash: H256::repeat_byte(3),
        state: EthTxState::Unresolved,
    };

    Transactions::<T>::insert(tx_id, tx_data);
}

fn setup_corroborations<T: Config>(tx_id: u32, num_success: u32, num_failure: u32) {
    let mut success_corroborations = BoundedVec::default();
    for i in 0..num_success {
        let author: [u8; 32] = [i as u8; 32];
        success_corroborations.try_push(author).unwrap();
    }

    let mut failure_corroborations = BoundedVec::default();
    for i in 0..num_failure {
        let author: [u8; 32] = [(i + num_success) as u8; 32];
        failure_corroborations.try_push(author).unwrap();
    }

    let corroboration_data =
        CorroborationData { success: success_corroborations, failure: failure_corroborations };

    Corroborations::<T>::insert(tx_id, corroboration_data);

    let unresolved_txs = vec![tx_id];
    let bounded_unresolved_txs = BoundedVec::try_from(unresolved_txs).unwrap();
    UnresolvedTxList::<T>::put(bounded_unresolved_txs);
}

benchmarks! {
    set_eth_tx_lifetime_secs {
        let eth_tx_lifetime_secs = 300u64;

    }: _(RawOrigin::Root, eth_tx_lifetime_secs)
    verify {
        assert_eq!(EthTxLifetimeSecs::<T>::get(), eth_tx_lifetime_secs);
    }

    add_confirmation {
        let c in 0 .. crate::CONFIRMATIONS_LIMIT - 1;
        let tx_id = 1u32;
        setup_tx_data::<T>(tx_id, c);
        let new_confirmation: [u8; 65] = [99u8; 65];

    }: _(RawOrigin::None, tx_id, new_confirmation)
    verify {
        let tx_data = Transactions::<T>::get(tx_id);
        ensure!(tx_data.confirmations.contains(&new_confirmation), "Confirmation not added");
    }

    add_corroboration {
        let tx_id = 1u32;
        let author: [u8; 32] = [4u8; 32];
        setup_tx_data::<T>(tx_id, 3);
        setup_corroborations::<T>(tx_id, 3, 3);
        let succeeded = true;

    }: _(RawOrigin::None, tx_id, author, succeeded)
    verify {
        let corroboration_data = Corroborations::<T>::get(tx_id);
        ensure!(corroboration_data.success.contains(&author), "Corroboration not added to successes");
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().as_externality(),
    crate::mock::TestRuntime,
);
