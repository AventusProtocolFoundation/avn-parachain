#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as AvnOracle;
use codec::{Decode, Encode};
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::{
    assert_ok,
    pallet_prelude::{ConstU32, Weight},
    traits::{Get, Hooks},
    BoundedVec,
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use scale_info::prelude::{format, vec, vec::Vec};
use sp_avn_common::event_types::Validator;
use sp_core::U256;
use sp_runtime::{RuntimeAppPublic, WeakBoundedVec};

fn generate_validators<T: Config>(count: usize) -> Vec<Validator<T::AuthorityId, T::AccountId>> {
    let mut validators = Vec::new();

    for i in 0..=count {
        let seed = format!("//benchmark_{}", i).as_bytes().to_vec();
        let authority_id = T::AuthorityId::generate_pair(Some(seed));

        // Create dummy AccountId (you can replace this logic with specific hardcoded keys if
        // needed)
        let account_seed = [i as u8; 32]; // use i to make unique
        let account_id = T::AccountId::decode(&mut &account_seed[..])
            .unwrap_or_else(|_| panic!("Failed to create AccountId from seed for validator {}", i));

        let validator = Validator { key: authority_id.clone(), account_id: account_id.clone() };

        validators.push(validator);
    }

    pallet_avn::Validators::<T>::put(WeakBoundedVec::force_from(
        validators.clone(),
        Some("Failed to set validators"),
    ));

    validators
}

fn register_n_currencies<T: Config>(n: u32) {
    for i in 0..n {
        let currency_symbol = format!("us{}", i).into_bytes();
        let currency = create_currency(currency_symbol.clone());
        Currencies::<T>::insert(currency, ());
    }
}

pub fn create_currency(currency_symbol: Vec<u8>) -> Currency {
    let currency = BoundedVec::<u8, ConstU32<{ MAX_CURRENCY_LENGTH }>>::try_from(currency_symbol)
        .expect("currency symbol must be ≤ MAX_CURRENCY_LENGTH bytes");
    currency
}

pub fn create_rates(rates: Vec<(Currency, U256)>) -> Rates {
    let bounded: Rates = rates.try_into().expect("number of rates must be ≤ MAX_RATES");
    bounded
}

benchmarks! {
    submit_price {
        let current_authors = generate_validators::<T>(10);

        let currency_symbol = b"usd".to_vec();
        let currency = create_currency(currency_symbol.clone());
        assert_ok!(AvnOracle::<T>::register_currency(RawOrigin::Root.into(), currency_symbol.clone(),));

        let rates = create_rates(vec![(currency, U256::from(1000))]);

        let context = (PRICE_SUBMISSION_CONTEXT, rates.clone(), VotingRoundId::<T>::get()).encode();
        let quorum = AVN::<T>::quorum() as usize;

        // Submit reports from the first 4 validators to simulate quorum preparation
        for i in 0..quorum {
            let signature = current_authors[i].key.sign(&context).expect("Valid signature");
            AvnOracle::<T>::submit_price(
                RawOrigin::None.into(),
                rates.clone(),
                current_authors[i].clone(),
                signature,
            )?;
        }
        // The main submission for benchmarking, this will trigger quorum
        let signature = current_authors[quorum].key.sign(&context).expect("Valid signature");
    }: _(RawOrigin::None, rates.clone(), current_authors[quorum].clone(), signature)
    verify {
        // Verify that all 5 validators have reported
        for i in 0..=quorum {
            assert!(PriceReporters::<T>::contains_key(0, &current_authors[i].account_id));
        }

         // Verify the reported rate count
        assert_eq!(ReportedRates::<T>::get(0, rates), (quorum + 1) as u32);

        // Ensure the voting_round_id incremented, indicating quorum was met
        assert_eq!(VotingRoundId::<T>::get(), 1);
    }

    register_currency {
        let m in 0 .. T::MaxCurrencies::get().saturating_sub(1);
        register_n_currencies::<T>(m);

        let currency_symbol = b"eur".to_vec();
    }: _(RawOrigin::Root, currency_symbol.clone())
    verify {
        let currency = create_currency(currency_symbol.clone());
        assert!(Currencies::<T>::contains_key(&currency));
    }

    remove_currency {
        let currency_symbol = b"usd".to_vec();
        let currency = create_currency(currency_symbol.clone());
        Currencies::<T>::insert(&currency, ());
    }: _(RawOrigin::Root, currency_symbol.clone())
    verify {
        assert!(!Currencies::<T>::contains_key(&currency));
    }

    clear_consensus {
        let validator = generate_validators::<T>(1)[0].clone();
        let context = (CLEAR_CONSENSUS_SUBMISSION_CONTEXT, VotingRoundId::<T>::get()).encode();
        let signature = validator.key.sign(&context).expect("Invalid signature");

        let current_block_with_expired_grace_period = RatesRefreshRangeBlocks::<T>::get() + T::ConsensusGracePeriod::get() + 1;
        let now = BlockNumberFor::<T>::from(current_block_with_expired_grace_period);
        frame_system::Pallet::<T>::set_block_number(now);

        let last_submission = BlockNumberFor::<T>::from(0u32);

        LastPriceSubmission::<T>::put(last_submission);
    }: _(RawOrigin::None, validator.clone(), signature)
    verify {
        let updated_voting_round_id = VotingRoundId::<T>::get();
        assert_eq!(updated_voting_round_id, 1);

        let stored_block = LastPriceSubmission::<T>::get();
        let new_last_submission = BlockNumberFor::<T>::from(current_block_with_expired_grace_period.saturating_sub(RatesRefreshRangeBlocks::<T>::get()));
        assert_eq!(stored_block, new_last_submission);
    }

    on_initialize_updates_rates_query_timestamps {
        // Set up a block that should trigger the timestamp update
        let last_block = BlockNumberFor::<T>::from(1u32);
        let current_block = last_block + BlockNumberFor::<T>::from(RatesRefreshRangeBlocks::<T>::get() + 1);

        LastPriceSubmission::<T>::put(last_block);

        let initial_timestamp: T::Moment = 50000000u64.try_into().unwrap_or_default();
        pallet_timestamp::Pallet::<T>::set_timestamp(initial_timestamp);

    }: { AvnOracle::<T>::on_initialize(current_block) }
    verify {
        let voting_round_id = VotingRoundId::<T>::get();
        let (from, to) = PriceSubmissionTimestamps::<T>::get(voting_round_id)
            .expect("Expected FiatRatesSubmissionTimestamps to contain a value");

        assert!(
            to == from.saturating_add(600),
            "Expected 'to' > 'from' but got from={:?} to={:?}",
            from, to
        );
    }

    on_initialize_without_updating_rates_query_timestamps {
        // Set up a block that should trigger the timestamp update
        let last_block = BlockNumberFor::<T>::from(1u32);
        LastPriceSubmission::<T>::put(last_block);

        let current_block = last_block + BlockNumberFor::<T>::from(1u32);

        let initial_timestamp: T::Moment = 50000000u64.try_into().unwrap_or_default();
        pallet_timestamp::Pallet::<T>::set_timestamp(initial_timestamp);

    }: { AvnOracle::<T>::on_initialize(current_block) }
    verify {
        let voting_round_id = VotingRoundId::<T>::get();
        // timestamps not set
        assert!(PriceSubmissionTimestamps::<T>::get(voting_round_id).is_none());
    }

    on_idle_one_full_iteration {
        let voting_round_id = 0u32;
        let current_authors = generate_validators::<T>(10);

        let currency = create_currency(b"usd".to_vec().clone());
        let rates = create_rates(vec![(currency, U256::from(1000))]);

        let quorum = AVN::<T>::quorum() as usize;
        for i in 0..=quorum {
            PriceReporters::<T>::insert(voting_round_id, &current_authors[i].account_id, ());
        }
        ReportedRates::<T>::insert(voting_round_id, rates, 5);
        ProcessedVotingRoundIds::<T>::put(voting_round_id + 1);

        let limit = Weight::from_parts(1_000_000_000_000_000, 1000000);
    }: { AvnOracle::<T>::on_idle(1u32.into(), limit) }
    verify {
        assert_eq!(LastClearedVotingRoundIds::<T>::get(), Some((1,1)));

        // Ensure storage maps are empty after cleanup
        assert!(PriceReporters::<T>::iter_prefix(voting_round_id).next().is_none());
        assert!(ReportedRates::<T>::iter_prefix(voting_round_id).next().is_none());
    }

    set_rates_refresh_range {
        let new_rates_refresh_range = 990;
        assert!(RatesRefreshRangeBlocks::<T>::get() != new_rates_refresh_range);
    }: _(RawOrigin::Root, new_rates_refresh_range)
    verify {
        assert_eq!(RatesRefreshRangeBlocks::<T>::get(), new_rates_refresh_range);
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().with_validators().as_externality(),
    crate::mock::TestRuntime,
);
