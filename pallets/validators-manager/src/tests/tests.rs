//Copyright 2025 Aventus Network Services (UK) Ltd.

#![cfg(test)]

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok, traits::Currency};
use hex_literal::hex;
use pallet_parachain_staking::Error as ParachainStakingError;
use sp_avn_common::assert_eq_uvec;
use sp_runtime::{testing::UintAuthorityId, traits::BadOrigin};

fn register_validator(
    collator_id: &AccountId,
    collator_eth_public_key: &ecdsa::Public,
) -> DispatchResult {
    ValidatorManager::add_collator(
        RawOrigin::Root.into(),
        *collator_id,
        *collator_eth_public_key,
        None,
    )
}

fn set_session_keys(collator_id: &AccountId) {
    pallet_session::NextKeys::<TestRuntime>::insert::<AccountId, UintAuthorityId>(
        *collator_id,
        UintAuthorityId(10u64).into(),
    );
}

fn force_add_collator(
    collator_id: &AccountId,
    collator_eth_public_key: &ecdsa::Public,
) -> DispatchResult {
    set_session_keys(collator_id);
    assert_ok!(register_validator(collator_id, collator_eth_public_key));

    // Simulate T1 callback to complete registration
    let pending = PendingValidatorRegistrations::<TestRuntime>::get(collator_id).unwrap();
    assert_ok!(ValidatorManager::process_result(pending.tx_id, b"author_manager".to_vec(), true,));

    //Advance 2 session to add the collator to the session
    advance_session();
    advance_session();

    Ok(())
}

#[test]
fn lydia_test_register_existing_validator() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        let mock_data = MockData::setup_valid();
        ValidatorManager::insert_to_validators(&mock_data.new_validator_id);

        let current_num_events = System::events().len();

        //Set the session keys of the new validator we are trying to register
        set_session_keys(&mock_data.new_validator_id);

        assert_noop!(
            register_validator(&mock_data.new_validator_id, &mock_data.collator_eth_public_key),
            Error::<TestRuntime>::ValidatorAlreadyExists
        );

        // No Event has been deposited
        assert_eq!(System::events().len(), current_num_events);
    });
}

#[test]
fn lydia_test_register_validator_with_no_validators() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let mock_data = MockData::setup_valid();
        let current_num_events = System::events().len();

        //Set the session keys of the new validator we are trying to register
        set_session_keys(&mock_data.new_validator_id);

        assert_noop!(
            register_validator(&mock_data.new_validator_id, &mock_data.collator_eth_public_key),
            Error::<TestRuntime>::NoValidators
        );

        // no Event has been deposited
        assert_eq!(System::events().len(), current_num_events);
    });
}

mod register_validator {
    use super::*;

    // TODO move MockData here and rename to Context

    fn run_preconditions(context: &MockData) {
        assert_eq!(0, ValidatorActions::<TestRuntime>::iter().count());
        let validator_account_ids =
            ValidatorManager::validator_account_ids().expect("Should contain validators");
        assert_eq!(false, validator_account_ids.contains(&context.new_validator_id));
        assert_eq!(
            false,
            ValidatorManager::get_ethereum_public_key_if_exists(&context.new_validator_id)
                .is_some()
        );
    }

    fn find_validator_activation_action(data: &MockData, status: ValidatorsActionStatus) -> bool {
        return ValidatorActions::<TestRuntime>::iter().any(|(account_id, _ingress, action_data)| {
            action_data.status == status &&
                action_data.action_type == ValidatorsActionType::Activation &&
                account_id == data.new_validator_id
        })
    }

    mod succeeds {
        use super::*;

        #[test]
        fn and_adds_validator() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = MockData::setup_valid();
                run_preconditions(&context);

                //set the session keys of the new validator we are trying to register
                set_session_keys(&context.new_validator_id);

                // Result OK
                assert_ok!(register_validator(
                    &context.new_validator_id,
                    &context.collator_eth_public_key
                ));

                // Async behavior: Validator should be in pending state
                assert!(PendingValidatorRegistrations::<TestRuntime>::contains_key(
                    &context.new_validator_id
                ));
                assert!(!ValidatorManager::validator_account_ids()
                    .unwrap()
                    .contains(&context.new_validator_id));

                // ValidatorRegistrationPending Event should be emitted
                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ValidatorManager(
                        crate::Event::<TestRuntime>::ValidatorRegistrationPending {
                            validator_id: context.new_validator_id,
                            eth_key: context.collator_eth_public_key.clone(),
                            tx_id: PendingValidatorRegistrations::<TestRuntime>::get(
                                &context.new_validator_id
                            )
                            .unwrap()
                            .tx_id,
                        }
                    )));

                // Simulate T1 callback success
                let pending =
                    PendingValidatorRegistrations::<TestRuntime>::get(&context.new_validator_id)
                        .unwrap();
                assert_ok!(ValidatorManager::process_result(
                    pending.tx_id,
                    b"author_manager".to_vec(),
                    true,
                ));

                // Now: Validator has been added to ValidatorAccountIds storage
                assert!(ValidatorManager::validator_account_ids()
                    .unwrap()
                    .contains(&context.new_validator_id));

                // ValidatorRegistered Event has been deposited
                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ValidatorManager(
                        crate::Event::<TestRuntime>::ValidatorRegistered {
                            validator_id: context.new_validator_id,
                            eth_key: context.collator_eth_public_key.clone()
                        }
                    )));

                // Activation action has been triggered
                assert!(find_validator_activation_action(
                    &context,
                    ValidatorsActionStatus::AwaitingConfirmation
                ));
            });
        }

        #[test]
        fn activation_dispatches_after_two_sessions() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = MockData::setup_valid();
                run_preconditions(&context);

                //Set the session keys of the new validator we are trying to register
                set_session_keys(&context.new_validator_id);

                assert_ok!(register_validator(
                    &context.new_validator_id,
                    &context.collator_eth_public_key
                ));

                // Simulate T1 callback success to complete registration
                let pending =
                    PendingValidatorRegistrations::<TestRuntime>::get(&context.new_validator_id)
                        .unwrap();
                assert_ok!(ValidatorManager::process_result(
                    pending.tx_id,
                    b"author_manager".to_vec(),
                    true,
                ));

                // After registration, activation action should be AwaitingConfirmation
                assert!(find_validator_activation_action(
                    &context,
                    ValidatorsActionStatus::AwaitingConfirmation
                ));

                // It takes 2 session for validators to be updated
                advance_session();

                // After first session, should be Confirmed
                assert!(find_validator_activation_action(
                    &context,
                    ValidatorsActionStatus::Confirmed
                ));

                advance_session();

                // ValidatorActivationStarted Event has been deposited
                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ValidatorManager(
                        crate::Event::<TestRuntime>::ValidatorActivationStarted {
                            validator_id: context.new_validator_id
                        }
                    )));
            });
        }
    }
}

// Change these tests to accomodate the use of votes
#[allow(non_fmt_panics)]
mod remove_validator_public {
    use super::*;

    // Tests for pub fn remove_validator(origin) -> DispatchResult {...}
    #[test]
    fn valid_case() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = MockData::setup_valid();
            assert_ok!(force_add_collator(
                &context.new_validator_id,
                &context.collator_eth_public_key
            ));

            //Prove this is an existing validator
            assert_eq_uvec!(
                Session::validators(),
                vec![
                    validator_id_1(),
                    validator_id_2(),
                    validator_id_3(),
                    validator_id_4(),
                    validator_id_5(),
                    context.new_validator_id
                ]
            );

            //Validator exists in the AVN
            assert_eq!(AVN::<TestRuntime>::is_validator(&context.new_validator_id), true);

            //Remove the validator
            assert_ok!(ValidatorManager::remove_validator(
                RawOrigin::Root.into(),
                context.new_validator_id
            ));

            // Async behavior: Validator should be in pending state
            assert!(PendingValidatorDeregistrations::<TestRuntime>::contains_key(
                &context.new_validator_id
            ));

            // Validator still in active list (waiting for T1 confirmation)
            assert!(ValidatorManager::validator_account_ids()
                .unwrap()
                .contains(&context.new_validator_id));

            // Simulate T1 callback success
            let pending =
                PendingValidatorDeregistrations::<TestRuntime>::get(&context.new_validator_id)
                    .unwrap();
            assert_ok!(ValidatorManager::process_result(
                pending.tx_id,
                b"author_manager".to_vec(),
                true,
            ));

            // After callback: Marked as deactivating
            assert!(DeactivatingValidators::<TestRuntime>::contains_key(&context.new_validator_id));
            assert!(!PendingValidatorDeregistrations::<TestRuntime>::contains_key(
                &context.new_validator_id
            ));

            //Validator is still in the session. Will be removed after unstaking completes.
            assert_eq_uvec!(
                Session::validators(),
                vec![
                    validator_id_1(),
                    validator_id_2(),
                    validator_id_3(),
                    validator_id_4(),
                    validator_id_5(),
                    context.new_validator_id
                ]
            );

            // Advance 2 sessions
            advance_session();
            advance_session();

            // Validator has been removed from the session
            assert_eq_uvec!(
                Session::validators(),
                vec![
                    validator_id_1(),
                    validator_id_2(),
                    validator_id_3(),
                    validator_id_4(),
                    validator_id_5()
                ]
            );

            //Validator is also removed from the AVN
            assert_eq!(AVN::<TestRuntime>::is_validator(&context.new_validator_id), false);
        });
    }

    #[test]
    fn fails_when_regular_sender_submits_transaction() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = MockData::setup_valid();
            assert_ok!(force_add_collator(
                &context.new_validator_id,
                &context.collator_eth_public_key
            ));

            let num_events = System::events().len();
            assert_noop!(
                ValidatorManager::remove_validator(
                    RuntimeOrigin::signed(validator_id_3()),
                    validator_id_3()
                ),
                BadOrigin
            );
            assert_eq!(System::events().len(), num_events);
        });
    }

    #[test]
    fn unsigned_sender() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = MockData::setup_valid();
            assert_ok!(force_add_collator(
                &context.new_validator_id,
                &context.collator_eth_public_key
            ));

            let num_events = System::events().len();
            assert_noop!(
                ValidatorManager::remove_validator(
                    RawOrigin::None.into(),
                    context.new_validator_id
                ),
                BadOrigin
            );
            assert_eq!(System::events().len(), num_events);
        });
    }

    #[test]
    fn non_validator() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            //Ensure we have enough candidates
            let context = MockData::setup_valid();
            assert_ok!(force_add_collator(
                &context.new_validator_id,
                &context.collator_eth_public_key
            ));

            let validator_account_id = TestAccount::new([0u8; 32]).account_id();
            let original_validators = ValidatorManager::validator_account_ids();
            let num_events = System::events().len();

            // Async behavior: Validation happens first, so ValidatorNotFound instead of
            // CandidateDNE
            assert_noop!(
                ValidatorManager::remove_validator(RawOrigin::Root.into(), validator_account_id),
                Error::<TestRuntime>::ValidatorNotFound
            );

            // Caller of remove function has to emit event if removal is successful.
            assert_eq!(System::events().len(), num_events);
            assert_eq!(ValidatorManager::validator_account_ids(), original_validators);
        });
    }
}

#[test]
fn lydia_test_initial_validators_populated_from_genesis_config() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        assert_eq!(
            ValidatorManager::validator_account_ids().unwrap(),
            genesis_config_initial_validators().to_vec()
        );
    });
}

// compress_public_key test module removed - compress_eth_public_key function was deleted from
// pallet

mod add_validator {
    use super::*;

    struct AddValidatorContext {
        collator: AccountId,
        collator_eth_public_key: ecdsa::Public,
    }

    impl Default for AddValidatorContext {
        fn default() -> Self {
            let collator = TestAccount::new([0u8; 32]).account_id();
            Balances::make_free_balance_be(&collator, 100000);

            AddValidatorContext {
                collator,
                collator_eth_public_key: ecdsa::Public::from_raw(hex!(
                    "02407b0d9f41148bbe3b6c7d4a62585ae66cc32a707441197fa5453abfebd31d57"
                )),
            }
        }
    }

    #[test]
    fn succeeds_with_good_parameters() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = &AddValidatorContext::default();

            set_session_keys(&context.collator);
            assert_ok!(register_validator(&context.collator, &context.collator_eth_public_key));

            // Async behavior: Validator should be in pending state, not active yet
            assert!(PendingValidatorRegistrations::<TestRuntime>::contains_key(&context.collator));
            assert!(!ValidatorManager::validator_account_ids()
                .unwrap()
                .contains(&context.collator));

            // Simulate T1 callback success
            let pending =
                PendingValidatorRegistrations::<TestRuntime>::get(&context.collator).unwrap();
            assert_ok!(ValidatorManager::process_result(
                pending.tx_id,
                b"author_manager".to_vec(),
                true,
            ));

            // Now validator should be active
            assert!(!PendingValidatorRegistrations::<TestRuntime>::contains_key(&context.collator));
            assert!(ValidatorManager::validator_account_ids().unwrap().contains(&context.collator));
            assert_eq!(
                ValidatorManager::get_validator_by_eth_public_key(
                    context.collator_eth_public_key.clone()
                )
                .unwrap(),
                context.collator
            );
        });
    }

    mod fails_when {
        use super::*;

        #[test]
        fn extrinsic_is_unsigned() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = &AddValidatorContext::default();

                set_session_keys(&context.collator);
                assert_noop!(
                    ValidatorManager::add_collator(
                        RawOrigin::None.into(),
                        context.collator,
                        context.collator_eth_public_key,
                        None
                    ),
                    BadOrigin
                );
            });
        }

        #[test]
        fn no_validators() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                // This test is passing because we are not using validators when building the test
                // extension
                let context = &AddValidatorContext::default();

                set_session_keys(&context.collator);
                assert_noop!(
                    register_validator(&context.collator, &context.collator_eth_public_key),
                    Error::<TestRuntime>::NoValidators
                );
            });
        }

        #[test]
        fn validator_eth_key_already_exists() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = &AddValidatorContext::default();

                set_session_keys(&context.collator);
                <EthereumPublicKeys<TestRuntime>>::insert(
                    context.collator_eth_public_key.clone(),
                    context.collator,
                );

                assert_noop!(
                    register_validator(&context.collator, &context.collator_eth_public_key),
                    Error::<TestRuntime>::ValidatorEthKeyAlreadyExists
                );
            });
        }

        #[test]
        fn validator_already_exists() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = &AddValidatorContext::default();

                set_session_keys(&context.collator);
                assert_ok!(<ValidatorAccountIds::<TestRuntime>>::try_append(&context.collator));

                assert_noop!(
                    register_validator(&context.collator, &context.collator_eth_public_key),
                    Error::<TestRuntime>::ValidatorAlreadyExists
                );
            });
        }

        #[test]
        fn maximum_collators_is_reached() {
            let mut ext = ExtBuilder::build_default().with_maximum_validators().as_externality();
            ext.execute_with(|| {
                let context = &AddValidatorContext::default();

                set_session_keys(&context.collator);
                assert_noop!(
                    register_validator(&context.collator, &context.collator_eth_public_key),
                    Error::<TestRuntime>::MaximumValidatorsReached
                );
            });
        }
    }
}

mod rotate_validator_ethereum_key {
    use sp_core::ByteArray;

    use super::*;

    struct RotateValidatorEthKeyContext {
        validator: AccountId,
        validator_eth_old_public_key: ecdsa::Public,
        validator_eth_new_public_key: ecdsa::Public,
    }

    impl Default for RotateValidatorEthKeyContext {
        fn default() -> Self {
            let validator = validator_id_1();
            Balances::make_free_balance_be(&validator, 100000);

            RotateValidatorEthKeyContext {
                validator,
                validator_eth_old_public_key: ecdsa::Public::from_slice(
                    &COLLATOR_1_ETHEREUM_PUPLIC_KEY,
                )
                .unwrap(),
                validator_eth_new_public_key: ecdsa::Public::from_raw(hex!(
                    "02407b0d9f41148bbe3b6c7d4a62585ae66cc32a707441197fa5453abfebd31d57"
                )),
            }
        }
    }

    #[test]
    fn succeeds_with_good_parameters() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = &RotateValidatorEthKeyContext::default();

            assert_ok!(ValidatorManager::rotate_validator_ethereum_key(
                RuntimeOrigin::root(),
                context.validator.clone(),
                context.validator_eth_old_public_key.clone(),
                context.validator_eth_new_public_key.clone()
            ));

            assert_eq!(
                true,
                ValidatorManager::validator_account_ids().unwrap().contains(&context.validator)
            );
            assert_eq!(
                ValidatorManager::get_validator_by_eth_public_key(
                    context.validator_eth_new_public_key.clone()
                )
                .unwrap(),
                context.validator
            );
            assert_eq!(
                ValidatorManager::get_validator_by_eth_public_key(
                    context.validator_eth_old_public_key.clone()
                ),
                None
            );
        });
    }

    mod fails_when {
        use super::*;

        #[test]
        fn extrinsic_is_unsigned() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = &RotateValidatorEthKeyContext::default();

                assert_noop!(
                    ValidatorManager::rotate_validator_ethereum_key(
                        RuntimeOrigin::none(),
                        context.validator.clone(),
                        context.validator_eth_old_public_key.clone(),
                        context.validator_eth_new_public_key.clone()
                    ),
                    BadOrigin
                );
            });
        }

        #[test]
        fn validator_eth_key_already_exists() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = &RotateValidatorEthKeyContext::default();

                assert_noop!(
                    ValidatorManager::rotate_validator_ethereum_key(
                        RuntimeOrigin::root(),
                        context.validator.clone(),
                        context.validator_eth_old_public_key.clone(),
                        ecdsa::Public::from_slice(&COLLATOR_2_ETHEREUM_PUPLIC_KEY).unwrap()
                    ),
                    Error::<TestRuntime>::ValidatorEthKeyAlreadyExists
                );
            });
        }

        #[test]
        fn validator_eth_key_unchanged() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = &RotateValidatorEthKeyContext::default();

                assert_noop!(
                    ValidatorManager::rotate_validator_ethereum_key(
                        RuntimeOrigin::root(),
                        context.validator.clone(),
                        context.validator_eth_old_public_key.clone(),
                        context.validator_eth_old_public_key.clone(),
                    ),
                    Error::<TestRuntime>::ValidatorEthKeyAlreadyExists
                );
            });
        }

        #[test]
        fn validator_not_found() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = &RotateValidatorEthKeyContext::default();

                let no_validator = TestAccount::new([6u8; 32]).account_id();

                assert_noop!(
                    ValidatorManager::rotate_validator_ethereum_key(
                        RuntimeOrigin::root(),
                        no_validator,
                        context.validator_eth_old_public_key.clone(),
                        context.validator_eth_new_public_key.clone()
                    ),
                    Error::<TestRuntime>::ValidatorNotFound
                );
            });
        }

        #[test]
        fn validator_keys_missmatch() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = &RotateValidatorEthKeyContext::default();

                assert_noop!(
                    ValidatorManager::rotate_validator_ethereum_key(
                        RuntimeOrigin::root(),
                        validator_id_5(),
                        context.validator_eth_old_public_key.clone(),
                        context.validator_eth_new_public_key.clone()
                    ),
                    Error::<TestRuntime>::ValidatorNotFound
                );
            });
        }
    }
}
