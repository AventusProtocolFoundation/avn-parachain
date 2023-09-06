//! # Avn finality tracker pallet
// Copyright 2022 Aventus Network Services (UK) Ltd.

//! avn-finality-tracker pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use crate::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use pallet_avn::{self as avn};
use sp_runtime::WeakBoundedVec;

fn setup<T: Config>(
    validators: &Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>>,
) -> (
    T::BlockNumber,
    Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
    <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature,
) {
    let new_finalised_block_number: T::BlockNumber = LatestFinalisedBlock::<T>::get() + 1u32.into();
    let validator: Validator<<T as avn::Config>::AuthorityId, T::AccountId> = validators[0].clone();
    let signature: <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature =
        generate_signature::<T>();

    (new_finalised_block_number, validator, signature)
}

fn setup_validators<T: Config>(
    number_of_validator_account_ids: u32,
) -> Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>> {
    let mnemonic: &str =
        "basic anxiety marine match castle rival moral whisper insane away avoid bike";
    let mut validators: Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>> =
        Vec::new();
    for i in 0..number_of_validator_account_ids {
        let account = account("dummy_validator", i, i);
        let key =
            <T as avn::Config>::AuthorityId::generate_pair(Some(mnemonic.as_bytes().to_vec()));
        validators.push(Validator::new(account, key));
    }

    // Setup validators in avn pallet
    avn::Validators::<T>::put(WeakBoundedVec::force_from(
        validators.clone(),
        Some("Too many validators for session"),
    ));

    return validators
}

fn generate_signature<T: pallet_avn::Config>(
) -> <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature {
    let encoded_data = 0.encode();
    let authority_id = T::AuthorityId::generate_pair(None);
    let signature = authority_id.sign(&encoded_data).expect("able to make signature");

    return signature
}

benchmarks! {
    submit_latest_finalised_block_number {
        let v in 3 .. MAX_VALIDATOR_ACCOUNT_IDS;

        let validators = setup_validators::<T>(v);
        let (new_finalised_block_number, validator, signature) = setup::<T>(&validators);
    }: _(RawOrigin::None, new_finalised_block_number, validator.clone(), signature)
    verify {
        let current_block_number = <frame_system::Pallet<T>>::block_number();
        assert_eq!(
            SubmittedBlockNumbers::<T>::get(&validator.account_id),
            SubmissionData::new(new_finalised_block_number, current_block_number)
        );
        assert_eq!(LastFinalisedBlockSubmission::<T>::get(), current_block_number);
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::TestExternalitiesBuilder::default().build(|| {}),
    crate::mock::TestRuntime,
);
