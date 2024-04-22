//! # Eth bridge pallet
// Copyright 2023 Aventus Network Services (UK) Ltd.

//! eth-bridge pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]
use crate::{avn::MAX_VALIDATOR_ACCOUNTS, Pallet, *};

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::{ensure, BoundedVec};
use frame_system::RawOrigin;
use hex_literal::hex;
use rand::{RngCore, SeedableRng};
use sp_core::H256;
use sp_runtime::WeakBoundedVec;

fn setup_authors<T: Config>(number_of_validator_account_ids: u32) -> Vec<crate::Author<T>> {
    let current_authors = avn::Validators::<T>::get();

    if number_of_validator_account_ids <= current_authors.len() as u32 {
        return current_authors.to_vec()
    }
    let number_of_new_authors: u32 = number_of_validator_account_ids - current_authors.len() as u32;
    let mut new_authors: Vec<crate::Author<T>> = Vec::new();
    for i in 0..number_of_new_authors {
        let account = account("dummy_validator", i, i);
        let key =
            <T as avn::Config>::AuthorityId::generate_pair(Some("//Ferdie".as_bytes().to_vec()));
        let _ = set_session_key::<T>(&account, current_authors.len() as u32 + i);
        new_authors.push(crate::Author::<T>::new(account, key));
    }

    let total_authors: Vec<_> = current_authors.iter().chain(new_authors.iter()).cloned().collect();

    // Setup authors in avn pallet
    avn::Validators::<T>::put(WeakBoundedVec::force_from(
        total_authors.clone(),
        Some("Too many authors for session"),
    ));

    return total_authors
}

fn add_collator_to_avn<T: Config>(
    collator: &T::AccountId,
    candidate_count: u32,
) -> Result<Validator<T::AuthorityId, T::AccountId>, &'static str> {
    let key = <T as avn::Config>::AuthorityId::generate_pair(Some("//Ferdie".as_bytes().to_vec()));
    let validator: Validator<T::AuthorityId, T::AccountId> =
        Validator::new(collator.clone(), key.into());

    let current_collators = avn::Validators::<T>::get();
    let new_collators: Vec<_> = current_collators
        .iter()
        .chain(vec![validator.clone()].iter())
        .cloned()
        .collect();

    avn::Validators::<T>::put(WeakBoundedVec::force_from(
        new_collators,
        Some("Too many validators for session"),
    ));

    set_session_key::<T>(collator, candidate_count)?;

    Ok(validator)
}

fn set_session_key<T: Config>(user: &T::AccountId, index: u32) -> Result<(), &'static str> {
    frame_system::Pallet::<T>::inc_providers(user);

    let keys = {
        let mut keys = [0u8; 128];
        let mut rng = rand::rngs::StdRng::seed_from_u64(index as u64);
        rng.fill_bytes(&mut keys);
        keys
    };

    let keys: T::Keys = Decode::decode(&mut &keys[..]).unwrap();

    pallet_session::Pallet::<T>::set_keys(
        RawOrigin::<T::AccountId>::Signed(user.clone()).into(),
        keys,
        Vec::new(),
    )?;

    Ok(())
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
    tx_id: EthereumId,
    num_confirmations: u32,
    sender: Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>,
    success_corroborations: BoundedVec<T::AccountId, ConstU32<MAX_CONFIRMATIONS>>,
    failure_corroborations: BoundedVec<T::AccountId, ConstU32<MAX_CONFIRMATIONS>>,
) {
    let expiry = 1438269973u64;
    let function_name =
        BoundedVec::<u8, crate::FunctionLimit>::try_from(b"sampleFunction".to_vec())
            .expect("Failed to create BoundedVec");

    let request_params = vec![(
        b"bytes32".to_vec(),
        hex::decode("30b83f0d722d1d4308ab4660a72dbaf0a7392d5674eca3cd21d57256d42df7a0").unwrap(),
    )];

    let mut params = request_params.clone();
    params.push((b"uint256".to_vec(), expiry.to_string().into_bytes()));
    params.push((b"uint32".to_vec(), tx_id.to_string().into_bytes()));

    let request_data = SendRequestData {
        tx_id,
        function_name: function_name.clone(),
        params: bound_params(request_params.to_vec()),
        caller_id: BoundedVec::<_, CallerIdLimit>::try_from(vec![]).unwrap(),
    };

    ActiveRequest::<T>::put(ActiveRequestData {
        request: Request::Send(request_data),
        confirmation: ActiveConfirmation {
            msg_hash: H256::repeat_byte(1),
            confirmations: {
                let mut confirmations = BoundedVec::default();
                for i in 0..num_confirmations {
                    let confirmation = generate_dummy_ecdsa_signature(i.try_into().unwrap());
                    confirmations.try_push(confirmation).unwrap();
                }
                confirmations
            },
        },
        tx_data: Some(ActiveEthTransaction {
            function_name: function_name.clone(),
            eth_tx_params: bound_params(params),
            sender: sender.account_id,
            expiry,
            eth_tx_hash: H256::zero(),
            success_corroborations,
            failure_corroborations,
            valid_tx_hash_corroborations: BoundedVec::default(),
            invalid_tx_hash_corroborations: BoundedVec::default(),
            tx_succeeded: false,
        }),
        last_updated: 0u32.into(),
    });
}

fn setup_new_active_tx<T: Config>(
    tx_id: EthereumId,
    num_confirmations: u32,
    sender: Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>,
) {
    setup_active_tx::<T>(
        tx_id,
        num_confirmations,
        sender,
        BoundedVec::default(),
        BoundedVec::default(),
    );
}

fn setup_active_tx_with_failure_corroborations<T: Config>(
    tx_id: EthereumId,
    num_confirmations: u32,
    sender: Author<T>,
    authors: Vec<Author<T>>,
    author: &Author<T>,
) {
    let mut local_authors: Vec<Author<T>> = authors.to_vec();
    local_authors.retain(|author_from_vec| author_from_vec.account_id != sender.account_id);
    local_authors.retain(|author_from_vec| author_from_vec.account_id != author.account_id);

    let (num_failure_corroborations, num_successful_corroborations) =
        get_num_corroborations::<T>(authors.len());

    let success_authors: Vec<T::AccountId> = local_authors
        .iter()
        .take(num_successful_corroborations)
        .map(|author| author.account_id.clone())
        .collect();

    let failure_authors: Vec<T::AccountId> = local_authors
        .iter()
        .skip(num_successful_corroborations)
        .take(num_failure_corroborations as usize)
        .map(|author| author.account_id.clone())
        .collect();

    setup_active_tx::<T>(
        tx_id,
        num_confirmations,
        sender,
        BoundedVec::try_from(success_authors).unwrap_or_default(),
        BoundedVec::try_from(failure_authors).unwrap_or_default(),
    );
}

fn get_num_corroborations<T: Config>(authors_count: usize) -> (usize, usize) {
    let quorum = avn::Pallet::<T>::quorum() as usize;

    let num_failure_corroborations = quorum - 1;

    let num_successful_corroborations = authors_count - num_failure_corroborations - 1; // because we are adding the last

    (num_failure_corroborations, num_successful_corroborations)
}

#[cfg(test)]
fn set_recovered_account_for_tests<T: Config>(sender_account_id: &T::AccountId) {
    let bytes = sender_account_id.encode();
    let mut vector: [u8; 8] = Default::default();
    vector.copy_from_slice(&bytes[0..8]);
    mock::set_mock_recovered_account_id(vector);
}

benchmarks! {
    set_eth_tx_lifetime_secs {
        let eth_tx_lifetime_secs = 300u64;
    }: _(RawOrigin::Root, eth_tx_lifetime_secs)
    verify {
        assert_eq!(EthTxLifetimeSecs::<T>::get(), eth_tx_lifetime_secs);
    }

    set_eth_tx_id {
        let eth_tx_id = 300u32;
    }: _(RawOrigin::Root, eth_tx_id)
    verify {
        assert_eq!(NextTxId::<T>::get(), eth_tx_id);
    }

    add_confirmation {
        let v in 1 .. MAX_CONFIRMATIONS;
        let authors = setup_authors::<T>(v + 4);

        let author: crate::Author<T> = authors[0].clone();
        let sender: crate::Author<T> = authors[1].clone();

        #[cfg(not(test))]
        let author = add_collator_to_avn::<T>(&author.account_id, authors.len() as u32 + 1u32)?;

        let quorum = avn::Pallet::<T>::quorum();
        let tx_id = 1u32;
        setup_new_active_tx::<T>(tx_id, quorum.saturating_sub(2), sender.clone());

        let active_tx = ActiveRequest::<T>::get().expect("is active");

        let new_confirmation: ecdsa::Signature = ecdsa::Signature::from_slice(&hex!("53ea27badd00d7b5e4d7e7eb2542ea3abfcd2d8014d2153719f3f00d4058c4027eac360877d5d191cbfdfe8cd72dfe82abc9192fc6c8dce21f3c6f23c43e053f1c")).unwrap().into();
        let proof = (crate::ADD_CONFIRMATION_CONTEXT, tx_id, new_confirmation.clone(), author.account_id.clone()).encode();

        let signature = author.key.sign(&proof).expect("Error signing proof");

        #[cfg(test)]
        set_recovered_account_for_tests::<T>(&author.account_id);

    }: _(RawOrigin::None, tx_id, new_confirmation.clone(), author.clone(), signature)
    verify {
        let active_tx = ActiveRequest::<T>::get().expect("is active");
        ensure!(active_tx.confirmation.confirmations.contains(&new_confirmation), "Confirmation not added");
    }

    add_eth_tx_hash {
        let authors = setup_authors::<T>(MAX_VALIDATOR_ACCOUNTS);
        let sender: crate::Author<T> = authors[0].clone();
        #[cfg(not(test))]
        let sender = add_collator_to_avn::<T>(&sender.account_id, authors.len() as u32 + 1u32)?;

        let tx_id = 2u32;
        setup_new_active_tx::<T>(tx_id, 1, sender.clone());
        let eth_tx_hash = H256::repeat_byte(1);
        let proof = (crate::ADD_ETH_TX_HASH_CONTEXT, tx_id, eth_tx_hash.clone(), sender.account_id.clone()).encode();
        let signature = sender.key.sign(&proof).expect("Error signing proof");
    }: _(RawOrigin::None, tx_id, eth_tx_hash.clone(), sender.clone(), signature)
    verify {
        let active_tx = ActiveRequest::<T>::get().expect("is active");
        assert_eq!(active_tx.tx_data.unwrap().eth_tx_hash, eth_tx_hash, "Eth tx hash not added");
    }

    add_corroboration {
        let authors = setup_authors::<T>(MAX_VALIDATOR_ACCOUNTS);

        let author: crate::Author<T> = authors[0].clone();
        let sender: crate::Author<T> = authors[1].clone();
        #[cfg(not(test))]
        let author = add_collator_to_avn::<T>(&author.account_id, authors.len() as u32 + 1u32)?;

        let tx_id = 3u32;
        setup_new_active_tx::<T>(tx_id, 1, sender.clone());
        let tx_succeeded = true;
        let tx_hash_valid = true;
        let proof = (crate::ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, author.account_id.clone()).encode();
        let signature = author.key.sign(&proof).expect("Error signing proof");
    }: add_corroboration(RawOrigin::None, tx_id, tx_succeeded, tx_hash_valid, author.clone(), signature)
    verify {
        let active_tx = ActiveRequest::<T>::get().expect("is active");
        ensure!(active_tx.tx_data.unwrap().success_corroborations.contains(&author.account_id), "Corroboration not added");
    }

    add_corroboration_with_challenge {
        let v in 4..MAX_VALIDATOR_ACCOUNTS;

        let authors = setup_authors::<T>(v);

        let author: crate::Author<T> = authors[0].clone();
        let sender: crate::Author<T> = authors[1].clone();
        #[cfg(not(test))]
        let author = add_collator_to_avn::<T>(&author.account_id, authors.len() as u32 + 1u32)?;

        let tx_id = 3u32;
        setup_active_tx_with_failure_corroborations::<T>(tx_id, 1, sender.clone(), authors.clone(), &author);
        let tx_succeeded = true;
        let tx_hash_valid = true;
        let proof = (crate::ADD_CORROBORATION_CONTEXT, tx_id, tx_succeeded, author.account_id.clone()).encode();
        let signature = author.key.sign(&proof).expect("Error signing proof");
    }: add_corroboration(RawOrigin::None, tx_id, tx_succeeded, tx_hash_valid, author.clone(), signature)
    verify {
        ensure!(SettledTransactions::<T>::get(tx_id).is_some(), "Transaction is not settled");
    }

    remove_active_request {
        let authors = setup_authors::<T>(2);
        let tx_id = 1u32;
        setup_new_active_tx::<T>(tx_id, 1, authors[1].clone());
        // Make sure there is an active request
        let _ = ActiveRequest::<T>::get().expect("is active");
    }: _(RawOrigin::Root)
    verify {
        ensure!(ActiveRequest::<T>::get().is_none(), "Active request not removed");
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().as_externality(),
    crate::mock::TestRuntime,
);
