#![cfg(test)]
use super::{AVN, *};
use crate::mock::*;
use frame_support::{assert_err, assert_ok, pallet_prelude::Weight, traits::Hooks};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::prelude::collections::HashSet;
use sp_core::{H160, U256};

fn submit_price_for_x_validators(num_validators: u64, rates: Rates) {
    for i in 1..=num_validators {
        let submitter = create_validator(i);
        let signature = generate_signature(&submitter, b"test context");

        assert_ok!(AvnOracle::submit_price(
            RuntimeOrigin::none(),
            rates.clone(),
            submitter.clone(),
            signature
        ));
    }
}

fn register_max_currencies() {
    let max_currencies: u32 = <TestRuntime as Config>::MaxCurrencies::get();
    for i in 1..=max_currencies {
        let currency = format!("us{}", i).into_bytes();
        let currency_symbol = CurrencySymbol(currency.clone());

        assert_ok!(AvnOracle::register_currency(RuntimeOrigin::root(), currency.clone(),));
        assert!(CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));
    }
}

fn submit_different_rates_for_x_validators(num_validators: u64) {
    for i in 1..=num_validators {
        let submitter = create_validator(i);
        let signature = generate_signature(&submitter, b"test context");
        let currency = CurrencySymbol(b"usd".to_vec());

        assert_ok!(AvnOracle::submit_price(
            RuntimeOrigin::none(),
            Rates(vec![(currency, U256::from(i),),]),
            submitter.clone(),
            signature
        ));
    }
}

#[cfg(test)]
mod price_submission {
    use super::*;

    #[cfg(test)]
    mod succeeds_if {
        use super::*;

        #[test]
        fn first_submission_by_validator() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let submitter = create_validator(1);
                let signature = generate_signature(&submitter, b"test context");
                let currency = CurrencySymbol(b"usd".to_vec());
                let rates = Rates(vec![(currency, U256::from(1000))]);

                let current_nonce = Nonce::<TestRuntime>::get();

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    rates.clone(),
                    submitter.clone(),
                    signature
                ));

                assert!(PriceReporters::<TestRuntime>::contains_key(
                    current_nonce,
                    &submitter.account_id
                ));
                let count = ReportedRates::<TestRuntime>::get(current_nonce, rates);
                assert_eq!(count, 1);
            });
        }

        #[test]
        fn second_submission_by_another_validator() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let submitter = create_validator(1);
                let submitter_2 = create_validator(2);
                let signature = generate_signature(&submitter, b"test context");
                let signature_2 = generate_signature(&submitter_2, b"test context");

                let currency = CurrencySymbol(b"usd".to_vec());
                let rates = Rates(vec![(currency, U256::from(1000))]);

                let current_nonce = Nonce::<TestRuntime>::get();

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    rates.clone(),
                    submitter.clone(),
                    signature
                ));
                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    rates.clone(),
                    submitter_2.clone(),
                    signature_2
                ));

                assert!(PriceReporters::<TestRuntime>::contains_key(
                    current_nonce,
                    &submitter.account_id
                ));
                assert!(PriceReporters::<TestRuntime>::contains_key(
                    current_nonce,
                    &submitter_2.account_id
                ));
                let count = ReportedRates::<TestRuntime>::get(current_nonce, rates);
                assert_eq!(count, 2);
            });
        }

        #[test]
        fn submission_with_multiple_currencies() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let submitter = create_validator(1);
                let signature = generate_signature(&submitter, b"test context");
                let usd = CurrencySymbol(b"usd".to_vec());
                let eur = CurrencySymbol(b"eur".to_vec());
                let rates = Rates(vec![(usd, U256::from(1000)), (eur, U256::from(1000))]);

                let current_nonce = Nonce::<TestRuntime>::get();

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    rates.clone(),
                    submitter.clone(),
                    signature
                ));

                assert!(PriceReporters::<TestRuntime>::contains_key(
                    current_nonce,
                    &submitter.account_id
                ));
                let count = ReportedRates::<TestRuntime>::get(current_nonce, rates);
                assert_eq!(count, 1);
            });
        }
    }

    #[cfg(test)]
    mod fails_if {
        use super::*;

        #[test]
        fn submitter_is_not_validator() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let submitter = create_validator(11);
                let signature = generate_signature(&submitter, b"test context");
                let currency = CurrencySymbol(b"usd".to_vec());
                let rates = Rates(vec![(currency, U256::from(1000))]);

                assert_err!(
                    AvnOracle::submit_price(
                        RuntimeOrigin::none(),
                        rates.clone(),
                        submitter.clone(),
                        signature
                    ),
                    Error::<TestRuntime>::SubmitterNotAValidator
                );
            });
        }

        #[test]
        fn second_submission_by_same_validator() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let submitter = create_validator(1);
                let signature = generate_signature(&submitter, b"test context");
                let currency = CurrencySymbol(b"usd".to_vec());
                let rates = Rates(vec![(currency, U256::from(1000))]);

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    rates.clone(),
                    submitter.clone(),
                    signature.clone()
                ));
                assert_err!(
                    AvnOracle::submit_price(
                        RuntimeOrigin::none(),
                        rates.clone(),
                        submitter.clone(),
                        signature
                    ),
                    Error::<TestRuntime>::ValidatorAlreadySubmitted
                );
            });
        }
    }

    #[cfg(test)]
    mod when_quorum_is_reached {
        use super::*;

        #[test]
        fn rates_event_is_emitted_and_storage_is_updated_correctly() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let number_of_validators = AVN::<TestRuntime>::quorum();
                let currency = CurrencySymbol(b"usd".to_vec());
                let rates = Rates(vec![(currency, U256::from(1000))]);

                let current_nonce = Nonce::<TestRuntime>::get();

                // add enough votes, just before quorum is reached
                submit_price_for_x_validators(number_of_validators.into(), rates.clone());

                // verify count
                let count = ReportedRates::<TestRuntime>::get(current_nonce, rates.clone());
                assert_eq!(count, number_of_validators);

                // verify nonce
                let nonce = Nonce::<TestRuntime>::get();
                assert_eq!(nonce, 0);

                // add the fifth validator that will trigger consensus
                let submitter = create_validator(5);
                let signature = generate_signature(&submitter, b"test context");
                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    rates.clone(),
                    submitter.clone(),
                    signature
                ));

                // check that nonce increments by currency
                assert_eq!(Nonce::<TestRuntime>::get(), 1);

                // check that price is updated
                for (symbol, value) in &rates.0 {
                    assert_eq!(
                        NativeTokenRateByCurrency::<TestRuntime>::get(symbol),
                        Some(value.clone())
                    );
                }

                // check that nonce has been processed for currency
                assert_eq!(ProcessedNonces::<TestRuntime>::get(), 0);

                // check that lastPriceSubmission updates correctly
                let current_block = <frame_system::Pallet<TestRuntime>>::block_number();
                assert_eq!(LastPriceSubmission::<TestRuntime>::get(), current_block);

                // event is emitted
                assert_eq!(
                    true,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::AvnOracle(
                            crate::Event::<TestRuntime>::RatesUpdated {
                                rates: rates.clone(),
                                nonce,
                            }
                        ))
                );
            });
        }
    }
}

#[cfg(test)]
mod register_currency {
    use super::*;

    #[cfg(test)]
    mod succeeds_if {
        use super::*;

        #[test]
        fn origin_is_sudo() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let currency = b"usd".to_vec();
                let currency_symbol = CurrencySymbol(currency.clone());

                // Ensure currency is not registered initially
                assert!(!CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Register currency
                assert_ok!(AvnOracle::register_currency(RuntimeOrigin::root(), currency.clone(),));

                // Ensure currency is added
                assert!(CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // event is emitted
                assert_eq!(
                    true,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::AvnOracle(
                            crate::Event::<TestRuntime>::CurrencyRegistered {
                                symbol: currency.clone(),
                            }
                        ))
                );
            });
        }
    }

    #[cfg(test)]
    mod fails_if {
        use super::*;

        #[test]
        fn origin_is_not_sudo() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let currency = b"usd".to_vec();
                let currency_symbol = CurrencySymbol(currency.clone());

                // Ensure currency is not registered initially
                assert!(!CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Register currency
                assert_err!(
                    AvnOracle::register_currency(RuntimeOrigin::signed(1), currency.clone(),),
                    sp_runtime::DispatchError::BadOrigin
                );

                // Ensure currency is not registered
                assert!(!CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Ensure no CurrencyRegistered event was emitted
                assert!(!System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::AvnOracle(
                        crate::Event::<TestRuntime>::CurrencyRegistered {
                            symbol: currency.clone(),
                        }
                    )));
            });
        }

        #[test]
        fn max_fiat_rates_reached() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                register_max_currencies();
                let currency = b"usd".to_vec();
                let currency_symbol = CurrencySymbol(currency.clone());

                // Ensure currency is not registered initially
                assert!(!CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Register currency
                assert_err!(
                    AvnOracle::register_currency(RuntimeOrigin::root(), currency.clone(),),
                    Error::<TestRuntime>::TooManyCurrencies
                );
            });
        }
    }
}

#[cfg(test)]
mod remove_currency {
    use super::*;

    #[cfg(test)]
    mod succeeds_if {
        use super::*;

        #[test]
        fn origin_is_sudo() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let currency = b"usd".to_vec();
                let currency_symbol = CurrencySymbol(currency.clone());

                // Ensure currency is not registered initially
                assert!(!CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Register currency
                assert_ok!(AvnOracle::register_currency(RuntimeOrigin::root(), currency.clone(),));

                // Ensure currency is added
                assert!(CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Remove currency
                assert_ok!(AvnOracle::remove_currency(RuntimeOrigin::root(), currency.clone(),));

                // Ensure currency is removed
                assert!(!CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // event is emitted
                assert_eq!(
                    true,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::AvnOracle(
                            crate::Event::<TestRuntime>::CurrencyRemoved {
                                symbol: currency.clone(),
                            }
                        ))
                );
            });
        }
    }

    #[cfg(test)]
    mod fails_if {
        use super::*;

        #[test]
        fn origin_is_not_sudo() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let currency = b"usd".to_vec();
                let currency_symbol = CurrencySymbol(currency.clone());

                // Ensure currency is not registered initially
                assert!(!CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Register currency
                assert_ok!(AvnOracle::register_currency(RuntimeOrigin::root(), currency.clone(),));

                // Ensure currency is added
                assert!(CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Remove currency
                assert_err!(
                    AvnOracle::remove_currency(RuntimeOrigin::signed(1), currency.clone(),),
                    sp_runtime::DispatchError::BadOrigin
                );

                // Ensure currency is not removed
                assert!(CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Ensure no CurrencyRemoved event was emitted
                assert!(!System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::AvnOracle(
                        crate::Event::<TestRuntime>::CurrencyRemoved { symbol: currency.clone() }
                    )));
            });
        }

        #[test]
        fn currency_not_found() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                register_max_currencies();
                let currency = b"usd".to_vec();
                let currency_symbol = CurrencySymbol(currency.clone());

                // Ensure currency is not registered initially
                assert!(!CurrencySymbols::<TestRuntime>::contains_key(&currency_symbol));

                // Remove currency
                assert_err!(
                    AvnOracle::remove_currency(RuntimeOrigin::root(), currency.clone(),),
                    Error::<TestRuntime>::CurrencyNotFound
                );
            });
        }
    }
}

#[cfg(test)]
mod clear_consensus {
    use super::*;

    #[cfg(test)]
    mod succeeds_if {
        use super::*;

        #[test]
        fn round_hasnt_finish_in_grace_period_blocks() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let number_of_validators = AVN::<TestRuntime>::quorum() + 1;
                let nonce = Nonce::<TestRuntime>::get();

                // add votes to all validators with different rates so consensus is never reached
                submit_different_rates_for_x_validators(number_of_validators.into());

                // round started at block 0
                assert_eq!(
                    LastPriceSubmission::<TestRuntime>::get(),
                    BlockNumberFor::<TestRuntime>::from(0u64)
                );

                // in grace period blocks, it should reset round
                let current_block: u64 = <frame_system::Pallet<TestRuntime>>::block_number();
                let rates_refresh_range: u32 =
                    <TestRuntime as Config>::PriceRefreshRangeInBlocks::get();
                let grace: u32 = <TestRuntime as Config>::ConsensusGracePeriod::get();
                let new_block_number = current_block
                    .saturating_add(grace.into())
                    .saturating_add(rates_refresh_range.into());
                System::set_block_number(new_block_number);

                let submitter = create_validator(1);
                let signature = generate_signature(&submitter, b"clear consensus");

                assert_ok!(Pallet::<TestRuntime>::clear_consensus(
                    RuntimeOrigin::none(),
                    submitter.clone(),
                    signature,
                ));

                // new submission round is set to rates_refresh_range blocks ago so it begins the
                // round right after
                let new_last_submission_block =
                    new_block_number.saturating_sub(rates_refresh_range.into());
                assert_eq!(LastPriceSubmission::<TestRuntime>::get(), new_last_submission_block);
                assert_eq!(Nonce::<TestRuntime>::get(), nonce + 1);
            })
        }
    }

    #[cfg(test)]
    mod fails_if {
        use super::*;

        #[test]
        fn submitter_is_not_a_validator() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let number_of_validators = AVN::<TestRuntime>::quorum() + 1;
                let nonce = Nonce::<TestRuntime>::get();

                // add votes to all validators with different rates so consensus is never reached
                submit_different_rates_for_x_validators(number_of_validators.into());

                // round started at block 0
                assert_eq!(
                    LastPriceSubmission::<TestRuntime>::get(),
                    BlockNumberFor::<TestRuntime>::from(0u64)
                );

                // in grace period blocks, it should reset round
                let current_block: u64 = <frame_system::Pallet<TestRuntime>>::block_number();
                let rates_refresh_range: u32 =
                    <TestRuntime as Config>::PriceRefreshRangeInBlocks::get();
                let grace: u32 = <TestRuntime as Config>::ConsensusGracePeriod::get();
                let new_block_number = current_block
                    .saturating_add(grace.into())
                    .saturating_add(rates_refresh_range.into());
                System::set_block_number(new_block_number);

                let submitter = create_validator(11);
                let signature = generate_signature(&submitter, b"clear consensus");

                assert_err!(
                    Pallet::<TestRuntime>::clear_consensus(
                        RuntimeOrigin::none(),
                        submitter.clone(),
                        signature,
                    ),
                    Error::<TestRuntime>::SubmitterNotAValidator
                );
            })
        }

        #[test]
        fn grace_period_has_not_passed() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let number_of_validators = AVN::<TestRuntime>::quorum() + 1;
                let nonce = Nonce::<TestRuntime>::get();

                // add votes to all validators with different rates so consensus is never reached
                submit_different_rates_for_x_validators(number_of_validators.into());

                // round started at block 0
                assert_eq!(
                    LastPriceSubmission::<TestRuntime>::get(),
                    BlockNumberFor::<TestRuntime>::from(0u64)
                );

                let current_block: u64 = <frame_system::Pallet<TestRuntime>>::block_number();
                let incomplete_grace: u32 =
                    <TestRuntime as Config>::ConsensusGracePeriod::get() - 5;
                System::set_block_number(current_block.saturating_add(incomplete_grace.into()));

                let submitter = create_validator(1);
                let signature = generate_signature(&submitter, b"clear rate");

                assert_err!(
                    Pallet::<TestRuntime>::clear_consensus(
                        RuntimeOrigin::none(),
                        submitter.clone(),
                        signature,
                    ),
                    Error::<TestRuntime>::GracePeriodNotPassed
                );
            })
        }
    }
}
