#![cfg(test)]
use super::{AVN, *};
use crate::mock::*;
// use env_logger;
use frame_support::{assert_err, assert_ok, pallet_prelude::Weight, traits::Hooks};
use scale_info::prelude::collections::HashSet;
use sp_core::{H160, U256};

fn submit_price_for_x_validators(num_validators: u64, currency: CurrencyBytes, price: U256) {
    for i in 1..=num_validators {
        let submitter = create_validator(i);
        let signature = generate_signature(&submitter, b"test context");

        assert_ok!(AvnOracle::submit_price(
            RuntimeOrigin::none(),
            currency.clone(),
            price,
            submitter.clone(),
            signature
        ));
    }
}

fn query_max_currencies() {
    let max_currencies = 10;
    for i in 0..max_currencies {
        let mut currency = b"us".to_vec();
        currency.extend_from_slice(i.to_string().as_bytes());

        assert_ok!(AvnOracle::query_currency(RuntimeOrigin::root(), currency.clone(),));
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
                let price = U256::from(1000);
                let currency = usd_key();
                let current_nonce = NonceByCurrency::<TestRuntime>::get(&currency);

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    currency.clone(),
                    price,
                    submitter.clone(),
                    signature
                ));

                assert!(PriceReporters::<TestRuntime>::contains_key(
                    (&currency, current_nonce),
                    &submitter.account_id
                ));
                let count = ReportedPrices::<TestRuntime>::get((&currency, current_nonce), price);
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

                let price = U256::from(1000);
                let currency = usd_key();
                let current_nonce = NonceByCurrency::<TestRuntime>::get(&currency);

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    currency.clone(),
                    price,
                    submitter.clone(),
                    signature
                ));

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    currency.clone(),
                    price,
                    submitter_2.clone(),
                    signature_2
                ));

                assert!(PriceReporters::<TestRuntime>::contains_key(
                    (&currency, current_nonce),
                    &submitter.account_id
                ));
                assert!(PriceReporters::<TestRuntime>::contains_key(
                    (&currency, current_nonce),
                    &submitter_2.account_id
                ));
                let count = ReportedPrices::<TestRuntime>::get((&currency, current_nonce), price);
                assert_eq!(count, 2);
            });
        }

        #[test]
        fn second_submission_by_same_validator_but_different_currency() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let submitter = create_validator(1);
                let signature = generate_signature(&submitter, b"test context");
                let signature_2 = generate_signature(&submitter, b"test context currency");

                let price = U256::from(1000);
                let usd = usd_key();
                let eur = eur_key();
                let nonce_eur = NonceByCurrency::<TestRuntime>::get(&eur);
                let nonce_usd = NonceByCurrency::<TestRuntime>::get(&usd);

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    usd.clone(),
                    price,
                    submitter.clone(),
                    signature
                ));

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    eur.clone(),
                    price,
                    submitter.clone(),
                    signature_2
                ));

                assert!(PriceReporters::<TestRuntime>::contains_key(
                    (&eur, nonce_eur),
                    &submitter.account_id
                ));
                assert!(PriceReporters::<TestRuntime>::contains_key(
                    (&usd, nonce_usd),
                    &submitter.account_id
                ));
                let count_eur = ReportedPrices::<TestRuntime>::get((&eur, nonce_eur), price);
                assert_eq!(count_eur, 1);

                let count_usd = ReportedPrices::<TestRuntime>::get((&usd, nonce_usd), price);
                assert_eq!(count_usd, 1);
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
                let price = U256::from(1000);
                let currency = usd_key();

                assert_err!(
                    AvnOracle::submit_price(
                        RuntimeOrigin::none(),
                        currency.clone(),
                        price,
                        submitter.clone(),
                        signature
                    ),
                    Error::<TestRuntime>::SubmitterNotAValidator
                );
            });
        }

        #[test]
        fn second_submission_by_same_validator_with_same_currency() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let submitter = create_validator(1);
                let signature = generate_signature(&submitter, b"test context");
                let price = U256::from(1000);
                let currency = usd_key();

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    currency.clone(),
                    price,
                    submitter.clone(),
                    signature.clone()
                ));
                assert_err!(
                    AvnOracle::submit_price(
                        RuntimeOrigin::none(),
                        currency.clone(),
                        price,
                        submitter.clone(),
                        signature
                    ),
                    Error::<TestRuntime>::ValidatorAlreadySubmitted
                );
            });
        }

        #[test]
        fn second_submission_by_same_validator_and_same_currency_but_different_prices() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let submitter = create_validator(1);
                let signature = generate_signature(&submitter, b"test context");
                let price = U256::from(1000);
                let different_price = U256::from(2000);
                let currency = usd_key();

                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    currency.clone(),
                    price,
                    submitter.clone(),
                    signature.clone()
                ));
                assert_err!(
                    AvnOracle::submit_price(
                        RuntimeOrigin::none(),
                        currency.clone(),
                        different_price,
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
        fn price_event_is_emitted_and_storage_is_updated_correctly() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let price = U256::from(1000);
                let number_of_validators = AVN::<TestRuntime>::quorum();
                let currency = usd_key();
                let nonce = NonceByCurrency::<TestRuntime>::get(&currency);

                // add enough votes, just before quorum is reached
                submit_price_for_x_validators(number_of_validators.into(), currency.clone(), price);

                // verify count
                let count = ReportedPrices::<TestRuntime>::get((&currency, nonce), price);
                assert_eq!(count, number_of_validators);

                // verify nonce
                assert_eq!(nonce, 0);

                // add the fifth validator that will trigger consensus
                let submitter = create_validator(5);
                let signature = generate_signature(&submitter, b"test context");
                assert_ok!(AvnOracle::submit_price(
                    RuntimeOrigin::none(),
                    currency.clone(),
                    price,
                    submitter.clone(),
                    signature
                ));

                // check that nonce increments by currency
                assert_eq!(NonceByCurrency::<TestRuntime>::get(&currency), 1);

                // check that price is updated
                assert_eq!(PricesByCurrency::<TestRuntime>::get(&currency), Some(price));

                // check that nonce has been processed for currency
                assert_eq!(ProcessedNonces::<TestRuntime>::get(&currency), 0);

                // event is emitted
                assert_eq!(
                    true,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::AvnOracle(
                            crate::Event::<TestRuntime>::PriceUpdated {
                                price,
                                currency: currency.clone(),
                                nonce,
                            }
                        ))
                );
            });
        }
    }
}

#[cfg(test)]
mod query_currency {
    use super::*;

    #[cfg(test)]
    mod succeeds_if {
        use super::*;

        #[test]
        fn currency_is_valid() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let currency_usd = b"usd".to_vec();

                assert_ok!(AvnOracle::query_currency(RuntimeOrigin::root(), currency_usd.clone(),));

                // ✅ Check that PendingCurrencies now contains "usd"
                let pending = AvnOracle::pending_currencies();
                assert_eq!(pending.len(), 1);
                assert_eq!(pending[0].as_slice(), currency_usd.as_slice());
            });
        }

        #[test]
        fn second_query_with_different_currency() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let currency_usd = b"usd".to_vec();
                let currency_eur = b"eur".to_vec();

                assert_ok!(AvnOracle::query_currency(RuntimeOrigin::root(), currency_usd.clone(),));

                assert_ok!(AvnOracle::query_currency(RuntimeOrigin::root(), currency_eur.clone(),));

                // ✅ Check that PendingCurrencies now contains "usd"
                let pending = AvnOracle::pending_currencies();
                assert_eq!(pending.len(), 2);
                assert_eq!(pending[0].as_slice(), currency_usd.as_slice());
                assert_eq!(pending[1].as_slice(), currency_eur.as_slice());
            });
        }
    }

    #[cfg(test)]
    mod fails_if {
        use super::*;

        #[test]
        fn currency_is_invalid() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let invalid_currency = b"usdd".to_vec();

                assert_err!(
                    AvnOracle::query_currency(RuntimeOrigin::root(), invalid_currency.clone(),),
                    Error::<TestRuntime>::InvalidCurrency
                );

                // ✅ Check that PendingCurrencies doesnt contain usd
                let pending = AvnOracle::pending_currencies();
                assert_eq!(pending.len(), 0);
            });
        }

        #[test]
        fn more_than_limit_currencies() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let max_currencies = 10;
                query_max_currencies();

                let currency = b"usd".to_vec();
                assert_err!(
                    AvnOracle::query_currency(RuntimeOrigin::root(), currency.clone(),),
                    Error::<TestRuntime>::TooManyCurrencies
                );

                // ✅ Check that PendingCurrencies doesnt contain more than max_currencies
                let pending = AvnOracle::pending_currencies();
                assert_eq!(pending.len(), max_currencies);
            });
        }
    }
}
