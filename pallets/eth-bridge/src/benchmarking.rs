//! # Eth bridge pallet
// Copyright 2023 Aventus Network Services (UK) Ltd.

//! eth-bridge pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use crate::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::{ensure, BoundedVec};
use frame_system::RawOrigin;
use sp_core::H256;
use sp_runtime::WeakBoundedVec;
use hex_literal::hex;

fn setup_authors<T: Config>(number_of_validator_account_ids: u32) -> Vec<crate::Author<T>> {
    let mnemonic: &str =
        "basic anxiety marine match castle rival moral whisper insane away avoid bike";
    let mut authors: Vec<crate::Author<T>> = Vec::new();
    for i in 0..number_of_validator_account_ids {
        let account = account("dummy_validator", i, i);
        let key =
            <T as avn::Config>::AuthorityId::generate_pair(Some(mnemonic.as_bytes().to_vec()));
        authors.push(crate::Author::<T>::new(account, key));
    }

    // Setup authors in avn pallet
    avn::Validators::<T>::put(WeakBoundedVec::force_from(
        authors.clone(),
        Some("Too many authors for session"),
    ));

    return authors
}

fn generate_dummy_ecdsa_signature(i: u8) -> ecdsa::Signature {
    let mut bytes: [u8; 65] = [0; 65];
    let first_64_bytes: [u8; 64] = [i; 64];
    bytes[0..64].copy_from_slice(&first_64_bytes);
    return ecdsa::Signature::from_raw(bytes)
}

fn bound_params(
    params: Vec<(Vec<u8>, Vec<u8>)>,
) -> BoundedVec<
    (BoundedVec<u8, crate::TypeLimit>, BoundedVec<u8, crate::ValueLimit>),
    crate::ParamsLimit,
> {
    let intermediate: Vec<_> = params
        .into_iter()
        .map(|(type_vec, value_vec)| {
            let type_bounded = BoundedVec::try_from(type_vec).expect("TypeNameLengthExceeded");
            let value_bounded = BoundedVec::try_from(value_vec).expect("ValueLengthExceeded");
            (type_bounded, value_bounded)
        })
        .collect();

    BoundedVec::<_, crate::ParamsLimit>::try_from(intermediate).expect("crate::ParamsLimitExceeded")
}

fn setup_active_tx<T: Config>(
    tx_id: u32,
    num_confirmations: u8,
    sender: Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>,
) {
    let expiry = 1438269973u64;
    let function_name =
        BoundedVec::<u8, crate::FunctionLimit>::try_from(b"sampleFunction".to_vec())
            .expect("Failed to create BoundedVec");

    let request_params = vec![
        (
            b"bytes32".to_vec(),
            hex::decode("30b83f0d722d1d4308ab4660a72dbaf0a7392d5674eca3cd21d57256d42df7a0")
                .unwrap(),
        )
    ];

    let mut params = request_params.clone();
    params.push((b"uint256".to_vec(), expiry.to_string().into_bytes()));
    params.push((b"uint32".to_vec(), tx_id.to_string().into_bytes()));

    let request_data = RequestData {
        tx_id,
        function_name: function_name.clone(),
        params: bound_params(request_params.to_vec()),
    };

    let tx_data = TransactionData {
        function_name,
        params: bound_params(params),
        sender: sender.account_id,
        eth_tx_hash: H256::zero(),
        tx_succeeded: false,
    };

    ActiveTransaction::<T>::put(ActiveTransactionData {
        id: tx_id,
        request_data,
        data: tx_data,
        expiry,
        msg_hash: H256::repeat_byte(1),
        confirmations: {
            let mut confirmations = BoundedVec::default();
            for i in 0..num_confirmations {
                let confirmation = generate_dummy_ecdsa_signature(i);
                confirmations.try_push(confirmation).unwrap();
            }
            confirmations
        },
        success_corroborations: BoundedVec::default(),
        failure_corroborations: BoundedVec::default(),
        valid_tx_hash_corroborations: BoundedVec::default(),
        invalid_tx_hash_corroborations: BoundedVec::default(),
    });
}

#[cfg(test)]
fn set_recovered_account_for_tests<T: Config>(sender_account_id: &T::AccountId) {
    let bytes = sender_account_id.encode();
    let mut vector: [u8; 8] = Default::default();
    vector.copy_from_slice(&bytes[0..8]);
    println!("set_recovered_account_for_tests {}", sender_account_id);
    mock::set_mock_recovered_account_id(vector);
}

benchmarks! {
    set_eth_tx_lifetime_secs {
        let eth_tx_lifetime_secs = 300u64;
    }: _(RawOrigin::Root, eth_tx_lifetime_secs)
    verify {
        assert_eq!(EthTxLifetimeSecs::<T>::get(), eth_tx_lifetime_secs);
    }

    add_confirmation {
        let authors = setup_authors::<T>(10);
        let author: crate::Author<T> = authors[0].clone();
        let sender: crate::Author<T> = authors[1].clone();
        let tx_id = 1u32;
        setup_active_tx::<T>(tx_id, 1, sender.clone());
        let active_tx = ActiveTransaction::<T>::get().expect("is active");
        let msg_hash_string = active_tx.msg_hash.to_string();
        // TODO: We need a real signature as this benchmark fails at present
        let new_confirmation: ecdsa::Signature = ecdsa::Signature::from_slice(&hex!("f825dec3df421141e5439f088c6cbd9db270fcfef21f15d48185883a817f60c451b5d9114feb1afd0e5489e08105dfd2887a775b96a1d7d3bdbfaec93e4f411b1b")).unwrap().into();
        let proof = (crate::ADD_CONFIRMATION_CONTEXT, tx_id, new_confirmation.clone(), author.account_id.clone()).encode();
        let signature = author.key.sign(&proof).expect("Error signing proof");

        #[cfg(test)]
        set_recovered_account_for_tests::<T>(&author.account_id);
    }: _(RawOrigin::None, tx_id, new_confirmation.clone(), author.clone(), signature)
    verify {
        let active_tx = ActiveTransaction::<T>::get().expect("is active");
        ensure!(active_tx.confirmations.contains(&new_confirmation), "Confirmation not added");
    }

    add_eth_tx_hash {
        let authors = setup_authors::<T>(10);
        let sender: crate::Author<T> = authors[0].clone();
        let tx_id = 2u32;
        setup_active_tx::<T>(tx_id, 1, sender.clone());
        let eth_tx_hash = H256::repeat_byte(1);
        let proof = (crate::ADD_ETH_TX_HASH_CONTEXT, tx_id, eth_tx_hash.clone(), sender.account_id.clone()).encode();
        let signature = sender.key.sign(&proof).expect("Error signing proof");
    }: _(RawOrigin::None, tx_id, eth_tx_hash.clone(), sender.clone(), signature)
    verify {
        let active_tx = ActiveTransaction::<T>::get().expect("is active");
        assert_eq!(active_tx.data.eth_tx_hash, eth_tx_hash, "Eth tx hash not added");
    }

    add_corroboration {
        let authors = setup_authors::<T>(10);
        let author: crate::Author<T> = authors[0].clone();
        let sender: crate::Author<T> = authors[1].clone();
        let tx_id = 3u32;
        setup_active_tx::<T>(tx_id, 1, sender.clone());
        let tx_succeeded = true;
        let tx_hash_valid = true;
        let proof = (crate::ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, author.account_id.clone()).encode();
        let signature = author.key.sign(&proof).expect("Error signing proof");
    }: _(RawOrigin::None, tx_id, tx_succeeded, tx_hash_valid, author.clone(), signature)
    verify {
        let active_tx = ActiveTransaction::<T>::get().expect("is active");
        ensure!(active_tx.success_corroborations.contains(&author.account_id), "Corroboration not added");
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().as_externality(),
    crate::mock::TestRuntime,
);
