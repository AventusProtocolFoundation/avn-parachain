//! # Eth bridge pallet
// Copyright 2023 Aventus Network Services (UK) Ltd.

//! eth-bridge pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use crate::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::{ensure, pallet_prelude::ConstU32, BoundedVec};
use frame_system::RawOrigin;
use sp_core::H256;
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

fn generate_dummy_ecdsa_signature(i: u8) -> ecdsa::Signature {
    let mut bytes: [u8; 65] = [0; 65];
    let first_64_bytes: [u8; 64] = [i; 64];
    bytes[0..64].copy_from_slice(&first_64_bytes);
    return ecdsa::Signature::from_raw(bytes)
}


fn bound_params(
    params: Vec<(Vec<u8>, Vec<u8>)>,
) -> BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit> {
    
    let intermediate: Vec<_> = params
        .into_iter()
        .map(|(type_vec, value_vec)| {
            let type_bounded = BoundedVec::try_from(type_vec)
                .expect("TypeNameLengthExceeded");
            let value_bounded = BoundedVec::try_from(value_vec)
                .expect("ValueLengthExceeded");
            (type_bounded, value_bounded)
        })
        .collect();

    BoundedVec::<_, ParamsLimit>::try_from(intermediate)
        .expect("ParamsLimitExceeded")
}

fn setup_tx_data<T: Config>(tx_id: u32, num_confirmations: u8, author: Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>) {
    let expiry = 438269973u64;
    let function_name = BoundedVec::<u8, FunctionLimit>::try_from(b"sampleFunction".to_vec()).expect("Failed to create BoundedVec");
    let params = vec![
        (b"bytes32".to_vec(), hex::decode("30b83f0d722d1d4308ab4660a72dbaf0a7392d5674eca3cd21d57256d42df7a0").unwrap()),
        (b"uint256".to_vec(), expiry.to_string().into_bytes()),
        (b"uint32".to_vec(), tx_id.to_string().into_bytes())
    ];

    let tx_data = TransactionData {
        function_name,
        params: bound_params(params),
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
        sender: author.account_id,
        eth_tx_hash: H256::zero(),
        status: EthTxStatus::Unresolved,
    };

    Transactions::<T>::insert(tx_id, tx_data);

    let corroborations = CorroborationData {
        eth_tx_succeeded: BoundedVec::default(),
        eth_tx_failed: BoundedVec::default(),
    };

    Corroborations::<T>::insert(tx_id, corroborations);
    
    let _ = UnresolvedTxList::<T>::try_mutate(|txs| {
        txs.try_push(tx_id)
    });
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
        setup_tx_data::<T>(tx_id, 1, author.clone());
        let tx_data = Transactions::<T>::get(tx_id).expect("Transaction should exist");
        let msg_hash_string = tx_data.msg_hash.to_string();
        let new_confirmation: ecdsa::Signature = ecdsa::Signature::from_slice(&hex!("3a0490e7d4325d3baa39b3011284e9758f9e370477e6b9e98713b2303da7427f71919f2757f62a01909391aeb3e89991539fdcb2d02ad45f7c64eb129c96f37100")).unwrap().into();
        let proof = (crate::ADD_CONFIRMATION_CONTEXT, tx_id, new_confirmation.clone(), author.account_id.clone()).encode();
        let signature = author.key.sign(&proof).expect("Error signing proof");
    }: _(RawOrigin::None, tx_id, new_confirmation.clone(), author.clone(), signature)
    verify {
        let tx_data = Transactions::<T>::get(tx_id).expect("Transaction should exist");
        ensure!(tx_data.confirmations.contains(&new_confirmation), "Confirmation not added");
    }

    add_receipt {
        let author = setup_author::<T>();
        let tx_id = 2u32;
        setup_tx_data::<T>(tx_id, 1, author.clone());
        let tx_data = Transactions::<T>::get(tx_id).expect("Transaction should exist");
        let eth_tx_hash = H256::repeat_byte(1);
        let proof = (crate::ADD_RECEIPT_CONTEXT, tx_id, eth_tx_hash.clone(), author.account_id.clone()).encode();
        let signature = author.key.sign(&proof).expect("Error signing proof");
    }: _(RawOrigin::None, tx_id, eth_tx_hash.clone(), author.clone(), signature)
    verify {
        let tx_data = Transactions::<T>::get(tx_id).expect("Transaction should exist");
        assert_eq!(tx_data.eth_tx_hash, eth_tx_hash, "Receipt not added");
    }

    add_corroboration {
        let author = setup_author::<T>();
        let tx_id = 3u32;
        setup_tx_data::<T>(tx_id, 1, author.clone());
        let eth_tx_succeeded = true;
        let proof = (crate::ADD_CORROBORATION_CONTEXT, tx_id, eth_tx_succeeded, author.account_id.clone()).encode();
        let signature = author.key.sign(&proof).expect("Error signing proof");
    }: _(RawOrigin::None, tx_id, eth_tx_succeeded, author.clone(), signature)
    verify {
        let corroboration = Corroborations::<T>::get(tx_id).unwrap();
        ensure!(corroboration.eth_tx_succeeded.contains(&author.account_id), "Corroboration not added");
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().as_externality(),
    crate::mock::TestRuntime,
);
