//Copyright 2022 Aventus Network Services (UK) Ltd.

#![cfg(test)]

use crate::{mock::*, *};
use frame_support::{
    assert_noop, assert_ok, pallet_prelude::DispatchResultWithPostInfo, traits::Currency,
};
use hex_literal::hex;
use pallet_parachain_staking::Error as ParachainStakingError;
use sp_io::crypto::{secp256k1_ecdsa_recover, secp256k1_ecdsa_recover_compressed};
use sp_runtime::{testing::UintAuthorityId, traits::BadOrigin};
use substrate_test_utils::assert_eq_uvec;
// use frame_system::*;

fn register_validator(
    collator_id: &AccountId,
    collator_eth_public_key: &ecdsa::Public,
) -> DispatchResultWithPostInfo {
    return ValidatorManager::add_collator(
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
                // Upon completion validator has been added ValidatorAccountIds storage
                assert!(ValidatorManager::validator_account_ids()
                    .unwrap()
                    .iter()
                    .any(|a| a == &context.new_validator_id));
                // ValidatorRegistered Event has been deposited
                assert_eq!(
                    true,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::ValidatorManager(
                            crate::Event::<TestRuntime>::ValidatorRegistered {
                                validator_id: context.new_validator_id,
                                eth_key: context.collator_eth_public_key.clone()
                            }
                        ))
                );
                // ValidatorActivationStarted Event has not been deposited yet
                assert_eq!(
                    false,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::ValidatorManager(
                            crate::Event::<TestRuntime>::ValidatorActivationStarted {
                                validator_id: context.new_validator_id
                            }
                        ))
                );
                // But the activation action has been triggered
                assert_eq!(
                    true,
                    find_validator_activation_action(
                        &context,
                        ValidatorsActionStatus::AwaitingConfirmation
                    )
                );
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

                // It takes 2 session for validators to be updated
                advance_session();
                advance_session();

                // The activation action has been sent
                assert_eq!(
                    true,
                    find_validator_activation_action(&context, ValidatorsActionStatus::Confirmed)
                );
                // ValidatorActivationStarted Event has been deposited
                assert_eq!(
                    true,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::ValidatorManager(
                            crate::Event::<TestRuntime>::ValidatorActivationStarted {
                                validator_id: context.new_validator_id
                            }
                        ))
                );
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

            //Event emitted as expected
            assert!(System::events().iter().any(|a| a.event ==
                mock::RuntimeEvent::ValidatorManager(
                    crate::Event::<TestRuntime>::ValidatorDeregistered {
                        validator_id: context.new_validator_id
                    }
                )));

            //Validator removed from validators manager
            assert_eq!(
                ValidatorManager::validator_account_ids()
                    .unwrap()
                    .iter()
                    .position(|&x| x == context.new_validator_id),
                None
            );

            //Validator is still in the session. Will be removed after 1 era.
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

            assert_noop!(
                ValidatorManager::remove_validator(RawOrigin::Root.into(), validator_account_id),
                ParachainStakingError::<TestRuntime>::CandidateDNE
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

mod compress_public_key {
    use super::*;

    fn dummy_ecdsa_signature_as_bytes(r: [u8; 32], s: [u8; 32], v: [u8; 1]) -> [u8; 65] {
        let mut sig = Vec::new();
        sig.extend_from_slice(&r);
        sig.extend_from_slice(&s);
        sig.extend_from_slice(&v);

        let mut result = [0; 65];
        result.copy_from_slice(&sig[..]);
        return result
    }

    mod returns_a_valid_public_key {
        use super::*;

        const MESSAGE: [u8; 32] = [10; 32];

        #[test]
        fn for_a_recovered_key_from_a_signature_with_v27() {
            let r = [1; 32];
            let s = [2; 32];
            let v = [27];
            let ecdsa_signature = dummy_ecdsa_signature_as_bytes(r, s, v);

            let uncompressed_pub_key =
                secp256k1_ecdsa_recover(&ecdsa_signature, &MESSAGE).map_err(|_| ()).unwrap();
            let expected_pub_key = secp256k1_ecdsa_recover_compressed(&ecdsa_signature, &MESSAGE)
                .map_err(|_| ())
                .unwrap();

            let calculated_pub_key =
                ValidatorManager::compress_eth_public_key(H512::from_slice(&uncompressed_pub_key));

            assert_eq!(ecdsa::Public::from_raw(expected_pub_key), calculated_pub_key);
        }

        #[test]
        fn for_a_recovered_key_from_a_signature_with_v28() {
            let r = [1; 32];
            let s = [2; 32];
            let v = [28];
            let ecdsa_signature = dummy_ecdsa_signature_as_bytes(r, s, v);

            let uncompressed_pub_key =
                secp256k1_ecdsa_recover(&ecdsa_signature, &MESSAGE).map_err(|_| ()).unwrap();
            let expected_pub_key = secp256k1_ecdsa_recover_compressed(&ecdsa_signature, &MESSAGE)
                .map_err(|_| ())
                .unwrap();

            let calculated_pub_key =
                ValidatorManager::compress_eth_public_key(H512::from_slice(&uncompressed_pub_key));

            assert_eq!(ecdsa::Public::from_raw(expected_pub_key), calculated_pub_key);
        }

        #[test]
        fn for_a_recovered_key_from_a_different_signature() {
            let r = [7; 32];
            let s = [9; 32];
            let v = [27];
            let ecdsa_signature = dummy_ecdsa_signature_as_bytes(r, s, v);

            let uncompressed_pub_key =
                secp256k1_ecdsa_recover(&ecdsa_signature, &MESSAGE).map_err(|_| ()).unwrap();
            let expected_pub_key = secp256k1_ecdsa_recover_compressed(&ecdsa_signature, &MESSAGE)
                .map_err(|_| ())
                .unwrap();

            let calculated_pub_key =
                ValidatorManager::compress_eth_public_key(H512::from_slice(&uncompressed_pub_key));

            assert_eq!(ecdsa::Public::from_raw(expected_pub_key), calculated_pub_key);
        }

        #[test]
        fn for_a_hard_coded_key() {
            // We must strip the `04` from the public key, otherwise it will not fit into a H512
            // This key is generated by running `scripts/eth/generate-ethereum-keys.js`
            let uncompressed_pub_key = hex!["8d5a0a0deb9db6775bcfe3f4d209efdb019e79682fd2bf81f1e325312dd1266ac9231db76588d67a7729c235ecd04a662dfb5d1bbfa19ebda5e601f3d373b5cf"];
            let expected_pub_key =
                hex!["038d5a0a0deb9db6775bcfe3f4d209efdb019e79682fd2bf81f1e325312dd1266a"];

            let calculated_pub_key =
                ValidatorManager::compress_eth_public_key(H512::from_slice(&uncompressed_pub_key));

            assert_eq!(ecdsa::Public::from_raw(expected_pub_key), calculated_pub_key);
        }
    }
}

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

            assert_eq!(
                true,
                ValidatorManager::validator_account_ids().unwrap().contains(&context.collator)
            );
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
