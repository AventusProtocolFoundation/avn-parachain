//! # Eth bridge pallet
// Copyright 2023 Aventus Network Services (UK) Ltd.

//! eth-bridge pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use crate::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::{ensure, pallet_prelude::ConstU32, BoundedVec};
use frame_system::RawOrigin;
use sp_runtime::WeakBoundedVec;

pub type FunctionLimit = ConstU32<{ crate::FUNCTION_NAME_CHAR_LIMIT }>;
pub type ParamsLimit = ConstU32<{ crate::PARAMS_LIMIT }>;
pub type TypeLimit = ConstU32<{ crate::TYPE_CHAR_LIMIT }>;
pub type ValueLimit = ConstU32<{ crate::VALUE_CHAR_LIMIT }>;

fn setup_author<T: Config>() -> Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId> {
    let mnemonic: &str =
        "basic anxiety marine match castle rival moral whisper insane away avoid bike";
    let account: T::AccountId = account("dummy_validator", 0, 0);
    let key = <T as avn::Config>::AuthorityId::generate_pair(Some(mnemonic.as_bytes().to_vec()));
    let validator = Validator::new(account, key);
    // Update the account id to a predefined one (Alice stash in this case)
    let account_bytes: [u8; 32] =
        hex_literal::hex!("be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25f");
    let account_id = T::AccountId::decode(&mut &account_bytes[..]).unwrap();
    let author = Validator::new(account_id, validator.key);
    // Setup validator in avn pallet
    avn::Validators::<T>::put(WeakBoundedVec::force_from(
        vec![author.clone()],
        Some("Too many validators for session"),
    ));

    author
}

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
        chosen_sender: Some([2u8; 32]),
        eth_tx_hash: H256::repeat_byte(3),
        status: EthTxStatus::Unresolved,
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

fn encode_add_confirmation_proof(tx_id: u32, confirmation: [u8; 65], author: [u8; 32]) -> Vec<u8> {
    return (crate::ADD_CONFIRMATION_CONTEXT, tx_id.clone(), confirmation, author.clone()).encode()
}

fn encode_add_corroboration_proof(tx_id: u32, succeeded: bool, author: [u8; 32]) -> Vec<u8> {
    return (crate::ADD_CORROBORATION_CONTEXT, tx_id.clone(), succeeded, author.clone()).encode()
}

benchmarks! {
    set_eth_tx_lifetime_secs {
        let eth_tx_lifetime_secs = 300u64;

    }: _(RawOrigin::Root, eth_tx_lifetime_secs)
    verify {
        assert_eq!(EthTxLifetimeSecs::<T>::get(), eth_tx_lifetime_secs);
    }

    add_confirmation {
        let author = setup_author::<T>();
        let tx_id = 1u32;
        let c in 0 .. crate::CONFIRMATIONS_LIMIT - 1;
        setup_tx_data::<T>(tx_id, c);
        let new_confirmation: [u8; 65] = [99u8; 65];
        let proof = encode_add_confirmation_proof(tx_id, new_confirmation, T::AccountToBytesConvert::into_bytes(&author.account_id));
        let signature = author.key.sign(&proof).expect("Error signing proof");
    }: _(RawOrigin::None, tx_id, new_confirmation, author, signature)
    verify {
        let tx_data = Transactions::<T>::get(tx_id);
        ensure!(tx_data.confirmations.contains(&new_confirmation), "Confirmation not added");
    }

    add_corroboration {
        let author = setup_author::<T>();
        let author_account_id = T::AccountToBytesConvert::into_bytes(&author.account_id);
        let tx_id = 1u32;
        setup_tx_data::<T>(tx_id, 3);
        setup_corroborations::<T>(tx_id, 3, 3);
        let succeeded = true;
        let proof = encode_add_corroboration_proof(tx_id, succeeded, author_account_id);
        let signature = author.key.sign(&proof).expect("Error signing proof");
    }: _(RawOrigin::None, tx_id, succeeded, author, signature)
    verify {
        let corroboration_data = Corroborations::<T>::get(tx_id);
        ensure!(corroboration_data.success.contains(&author_account_id), "Corroboration not added to successes");
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().as_externality(),
    crate::mock::TestRuntime,
);
