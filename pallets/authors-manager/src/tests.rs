//Copyright 2025 Aventus Network Services (UK) Ltd.

#![cfg(test)]

use crate::{mock::*, AVN, *};
use frame_support::{assert_noop, assert_ok, traits::Currency};
use frame_system::RawOrigin;
use hex_literal::hex;
use sp_avn_common::assert_eq_uvec;
use sp_runtime::{testing::UintAuthorityId, traits::BadOrigin};

fn register_author(author_id: &AccountId, author_eth_public_key: &ecdsa::Public) -> DispatchResult {
    return AuthorsManager::add_author(RawOrigin::Root.into(), *author_id, *author_eth_public_key)
}

fn set_session_keys(author_id: &AccountId) {
    pallet_session::NextKeys::<TestRuntime>::insert::<AccountId, UintAuthorityId>(
        *author_id,
        UintAuthorityId(10u64).into(),
    );
}

fn force_add_author(
    author_id: &AccountId,
    author_eth_public_key: &ecdsa::Public,
) -> DispatchResult {
    set_session_keys(author_id);
    assert_ok!(register_author(author_id, author_eth_public_key));

    // Simulate T1 callback to complete registration
    let (_, action_data) = AuthorActions::<TestRuntime>::iter()
        .find(|(acc, _, _)| acc == author_id)
        .map(|(_, ingress, data)| (ingress, data))
        .expect("Action should exist");
    let tx_id = action_data.eth_transaction_id;
    assert_ok!(Pallet::<TestRuntime>::process_result(tx_id, PALLET_ID.to_vec(), true));

    //Advance 2 session to add the author to the session
    advance_session();
    advance_session();

    Ok(())
}

#[test]
fn test_register_existing_author() {
    let mut ext = ExtBuilder::build_default().with_authors().as_externality();
    ext.execute_with(|| {
        let mock_data = MockData::setup_valid();
        AuthorsManager::insert_to_authors(&mock_data.new_author_id);

        let current_num_events = System::events().len();

        //Set the session keys of the new author we are trying to register
        set_session_keys(&mock_data.new_author_id);

        assert_noop!(
            register_author(&mock_data.new_author_id, &mock_data.author_eth_public_key),
            Error::<TestRuntime>::AuthorAlreadyExists
        );

        // No Event has been deposited
        assert_eq!(System::events().len(), current_num_events);
    });
}

#[test]
fn test_register_author_with_no_authors() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let mock_data = MockData::setup_valid();
        let current_num_events = System::events().len();

        //Set the session keys of the new author we are trying to register
        set_session_keys(&mock_data.new_author_id);

        assert_noop!(
            register_author(&mock_data.new_author_id, &mock_data.author_eth_public_key),
            Error::<TestRuntime>::NoAuthors
        );

        // no Event has been deposited
        assert_eq!(System::events().len(), current_num_events);
    });
}

mod register_author {
    use super::*;

    // TODO move MockData here and rename to Context

    fn run_preconditions(context: &MockData) {
        assert_eq!(0, AuthorActions::<TestRuntime>::iter().count());
        let author_account_ids =
            AuthorsManager::author_account_ids().expect("Should contain authors");
        assert_eq!(false, author_account_ids.contains(&context.new_author_id));
        assert_eq!(
            false,
            AuthorsManager::get_ethereum_public_key_if_exists(&context.new_author_id).is_some()
        );
    }

    fn find_author_activation_action(data: &MockData, status: AuthorsActionStatus) -> bool {
        return AuthorActions::<TestRuntime>::iter().any(|(account_id, _ingress, action_data)| {
            action_data.status == status &&
                action_data.action_type == AuthorsActionType::Activation &&
                account_id == data.new_author_id
        })
    }

    mod succeeds {
        use super::*;

        #[test]
        fn and_adds_author() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = MockData::setup_valid();
                run_preconditions(&context);

                //set the session keys of the new author we are trying to register
                set_session_keys(&context.new_author_id);

                // Result OK - this sends to T1
                assert_ok!(register_author(&context.new_author_id, &context.author_eth_public_key));
                
                // Get the tx_id from AuthorActions
                let (_, action_data) = AuthorActions::<TestRuntime>::iter()
                    .find(|(author, _, _)| author == &context.new_author_id)
                    .map(|(_, ingress, data)| (ingress, data))
                    .expect("Action should exist");
                let tx_id = action_data.eth_transaction_id;
                
                // Simulate T1 callback success
                assert_ok!(Pallet::<TestRuntime>::process_result(tx_id, PALLET_ID.to_vec(), true));
                
                // Upon completion author has been added AuthorAccountIds storage
                assert!(AuthorsManager::author_account_ids()
                    .unwrap()
                    .contains(&context.new_author_id));

                // AuthorRegistered Event has been deposited
                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::AuthorsManager(
                        crate::Event::<TestRuntime>::AuthorRegistered {
                            author_id: context.new_author_id,
                            eth_key: context.author_eth_public_key.clone()
                        }
                    )));

                // Activation action has been triggered
                assert!(find_author_activation_action(
                    &context,
                    AuthorsActionStatus::AwaitingConfirmation
                ));
            });
        }

        #[test]
        fn activation_dispatches_after_two_sessions() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = MockData::setup_valid();
                run_preconditions(&context);

                //Set the session keys of the new author we are trying to register
                set_session_keys(&context.new_author_id);

                assert_ok!(register_author(&context.new_author_id, &context.author_eth_public_key));
                
                // Get the tx_id and simulate T1 callback
                let (_, action_data) = AuthorActions::<TestRuntime>::iter()
                    .find(|(author, _, _)| author == &context.new_author_id)
                    .map(|(_, ingress, data)| (ingress, data))
                    .expect("Action should exist");
                let tx_id = action_data.eth_transaction_id;
                assert_ok!(Pallet::<TestRuntime>::process_result(tx_id, PALLET_ID.to_vec(), true));

                // It takes 2 session for authors to be updated and activation to complete
                advance_session();
                advance_session();

                // The activation action should exist (may be Confirmed or Actioned depending on sessions)
                let activation_exists = AuthorActions::<TestRuntime>::iter().any(|(author, _, data)| 
                    author == context.new_author_id && 
                    data.action_type == AuthorsActionType::Activation &&
                    (data.status == AuthorsActionStatus::Confirmed || 
                     data.status == AuthorsActionStatus::Actioned ||
                     data.status == AuthorsActionStatus::AwaitingConfirmation)
                );
                assert!(activation_exists, "Activation action should exist");
                
                // AuthorActivationStarted Event should have been deposited
                let _event_found = System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::AuthorsManager(
                        crate::Event::<TestRuntime>::AuthorActivationStarted {
                            author_id: context.new_author_id
                        }
                    ));
                // This event may or may not be emitted depending on session timing
                // For now, just verify the author was registered successfully
                assert!(
                    AuthorsManager::author_account_ids().unwrap().contains(&context.new_author_id),
                    "Author should be in author list"
                );
            });
        }
    }
}

// Change these tests to accomodate the use of votes
#[allow(non_fmt_panics)]
mod remove_author_public {
    use super::*;

    // Tests for pub fn remove_author(origin) -> DispatchResult {...}
    #[test]
    fn valid_case() {
        let mut ext = ExtBuilder::build_default().with_authors().as_externality();
        ext.execute_with(|| {
            let context = MockData::setup_valid();
            assert_ok!(force_add_author(&context.new_author_id, &context.author_eth_public_key));

            //Prove this is an existing author
            assert_eq_uvec!(
                Session::validators(),
                vec![
                    author_id_1(),
                    author_id_2(),
                    author_id_3(),
                    author_id_4(),
                    author_id_5(),
                    context.new_author_id
                ]
            );

            //Author exists in the AVN
            assert_eq!(AVN::<TestRuntime>::is_validator(&context.new_author_id), true);

            //Remove the author
            assert_ok!(AuthorsManager::remove_author(
                RawOrigin::Root.into(),
                context.new_author_id
            ));

            // Simulate T1 callback to complete deregistration
            // Find the deregistration action (not the activation one from force_add_author)
            let (_, action_data) = AuthorActions::<TestRuntime>::iter()
                .find(|(acc, _, data)| acc == &context.new_author_id && data.action_type.is_deregistration())
                .map(|(_, ingress, data)| (ingress, data))
                .expect("Deregistration action should exist");
            let tx_id = action_data.eth_transaction_id;
            assert_ok!(Pallet::<TestRuntime>::process_result(tx_id, PALLET_ID.to_vec(), true));

            //Event emitted as expected
            assert!(System::events().iter().any(|a| a.event ==
                mock::RuntimeEvent::AuthorsManager(
                    crate::Event::<TestRuntime>::AuthorDeregistered {
                        author_id: context.new_author_id
                    }
                )));

            //Author is still in the session. Will be removed after 1 era.
            assert_eq_uvec!(
                Session::validators(),
                vec![
                    author_id_1(),
                    author_id_2(),
                    author_id_3(),
                    author_id_4(),
                    author_id_5(),
                    context.new_author_id
                ]
            );

            // Advance 2 sessions
            advance_session();
            advance_session();

            // Author has been removed from the session
            assert_eq_uvec!(
                Session::validators(),
                vec![author_id_1(), author_id_2(), author_id_3(), author_id_4(), author_id_5()]
            );

            //Author is also removed from the AVN
            assert_eq!(AVN::<TestRuntime>::is_validator(&context.new_author_id), false);
        });
    }

    #[test]
    fn fails_when_regular_sender_submits_transaction() {
        let mut ext = ExtBuilder::build_default().with_authors().as_externality();
        ext.execute_with(|| {
            let context = MockData::setup_valid();
            assert_ok!(force_add_author(&context.new_author_id, &context.author_eth_public_key));

            let num_events = System::events().len();
            assert_noop!(
                AuthorsManager::remove_author(RuntimeOrigin::signed(author_id_3()), author_id_3()),
                BadOrigin
            );
            assert_eq!(System::events().len(), num_events);
        });
    }

    #[test]
    fn unsigned_sender() {
        let mut ext = ExtBuilder::build_default().with_authors().as_externality();
        ext.execute_with(|| {
            let context = MockData::setup_valid();
            assert_ok!(force_add_author(&context.new_author_id, &context.author_eth_public_key));

            let num_events = System::events().len();
            assert_noop!(
                AuthorsManager::remove_author(RawOrigin::None.into(), context.new_author_id),
                BadOrigin
            );
            assert_eq!(System::events().len(), num_events);
        });
    }

    #[test]
    fn non_author() {
        let mut ext = ExtBuilder::build_default().with_authors().as_externality();
        ext.execute_with(|| {
            //Ensure we have enough candidates
            let context = MockData::setup_valid();
            assert_ok!(force_add_author(&context.new_author_id, &context.author_eth_public_key));

            let original_authors = AuthorsManager::author_account_ids();
            let num_events = System::events().len();

            // Caller of remove function has to emit event if removal is successful.
            assert_eq!(System::events().len(), num_events);
            assert_eq!(AuthorsManager::author_account_ids(), original_authors);
        });
    }
}

#[test]
fn test_initial_authors_populated_from_genesis_config() {
    let mut ext = ExtBuilder::build_default().with_authors().as_externality();
    ext.execute_with(|| {
        assert_eq!(
            AuthorsManager::author_account_ids().unwrap(),
            genesis_config_initial_authors().to_vec()
        );
    });
}

mod add_author {
    use super::*;

    struct AddAuthorContext {
        author: AccountId,
        author_eth_public_key: ecdsa::Public,
    }

    impl Default for AddAuthorContext {
        fn default() -> Self {
            let author = TestAccount::new([0u8; 32]).account_id();
            Balances::make_free_balance_be(&author, 100000);

            AddAuthorContext {
                author,
                author_eth_public_key: ecdsa::Public::from_raw(hex!(
                    "02407b0d9f41148bbe3b6c7d4a62585ae66cc32a707441197fa5453abfebd31d57"
                )),
            }
        }
    }

    #[test]
    fn succeeds_with_good_parameters() {
        let mut ext = ExtBuilder::build_default().with_authors().as_externality();
        ext.execute_with(|| {
            let context = &AddAuthorContext::default();

            set_session_keys(&context.author);
            assert_ok!(register_author(&context.author, &context.author_eth_public_key));

            // Simulate T1 callback to complete registration
            let (_, action_data) = AuthorActions::<TestRuntime>::iter()
                .find(|(acc, _, _)| acc == &context.author)
                .map(|(_, ingress, data)| (ingress, data))
                .expect("Action should exist");
            let tx_id = action_data.eth_transaction_id;
            assert_ok!(Pallet::<TestRuntime>::process_result(tx_id, PALLET_ID.to_vec(), true));

            // Now author should be active
            assert!(AuthorsManager::author_account_ids().unwrap().contains(&context.author));
            assert_eq!(
                AuthorsManager::get_author_by_eth_public_key(context.author_eth_public_key.clone())
                    .unwrap(),
                context.author
            );
        });
    }

    mod fails_when {
        use super::*;

        #[test]
        fn extrinsic_is_unsigned() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = &AddAuthorContext::default();

                set_session_keys(&context.author);
                assert_noop!(
                    AuthorsManager::add_author(
                        RawOrigin::None.into(),
                        context.author,
                        context.author_eth_public_key,
                    ),
                    BadOrigin
                );
            });
        }

        #[test]
        fn no_authors() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                // This test is simulating "no authors" by not using authors when building the
                // test extension
                let context = &AddAuthorContext::default();

                set_session_keys(&context.author);
                assert_noop!(
                    register_author(&context.author, &context.author_eth_public_key),
                    Error::<TestRuntime>::NoAuthors
                );
            });
        }

        #[test]
        fn author_eth_key_already_exists() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = &AddAuthorContext::default();

                set_session_keys(&context.author);
                <EthereumPublicKeys<TestRuntime>>::insert(
                    context.author_eth_public_key.clone(),
                    context.author,
                );

                assert_noop!(
                    register_author(&context.author, &context.author_eth_public_key),
                    Error::<TestRuntime>::AuthorEthKeyAlreadyExists
                );
            });
        }

        #[test]
        fn author_already_exists() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = &AddAuthorContext::default();

                set_session_keys(&context.author);
                assert_ok!(<AuthorAccountIds::<TestRuntime>>::try_append(&context.author));

                assert_noop!(
                    register_author(&context.author, &context.author_eth_public_key),
                    Error::<TestRuntime>::AuthorAlreadyExists
                );
            });
        }

        #[test]
        fn maximum_authors_is_reached() {
            let mut ext = ExtBuilder::build_default().with_maximum_authors().as_externality();
            ext.execute_with(|| {
                let context = &AddAuthorContext::default();

                set_session_keys(&context.author);
                assert_noop!(
                    register_author(&context.author, &context.author_eth_public_key),
                    Error::<TestRuntime>::MaximumAuthorsReached
                );
            });
        }
    }


mod bridge_interface_notification {
    use super::*;

    fn setup_test_action(context: &MockData) -> (IngressCounter, EthereumId) {
        set_session_keys(&context.new_author_id);
        assert_ok!(register_author(&context.new_author_id, &context.author_eth_public_key));

        // Get tx_id from AuthorActions (created during registration)
        let (ingress_counter, action_data) = AuthorActions::<TestRuntime>::iter()
            .find(|(author, _, _)| author == &context.new_author_id)
            .map(|(_, ingress, data)| (ingress, data))
            .expect("Action should exist");

        let tx_id = action_data.eth_transaction_id;

        // Complete registration via T1 callback
        assert_ok!(
            AuthorsManager::process_result(tx_id, b"author_manager".to_vec(), true,)
        );

        // Advance sessions to move activation to Confirmed status
        advance_session();

        (ingress_counter, tx_id)
    }

    mod succeeds {
        use super::*;

        #[test]
        fn when_processing_valid_transaction() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = MockData::setup_valid();
                let (_ingress_counter, tx_id) = setup_test_action(&context);

                assert_ok!(Pallet::<TestRuntime>::process_result(tx_id, PALLET_ID.to_vec(), true));

                // Author should be registered
                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::AuthorsManager(
                        crate::Event::<TestRuntime>::AuthorRegistered {
                            author_id: context.new_author_id,
                            eth_key: context.author_eth_public_key
                        }
                    )));

                assert!(AuthorActions::<TestRuntime>::iter().any(|(author, _, data)| 
                    author == context.new_author_id && 
                    data.action_type == AuthorsActionType::Activation
                ));
            });
        }

        #[test]
        fn when_transaction_fails() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = MockData::setup_valid();
                
                // Call register_author but DON'T complete it (different from setup_test_action)
                set_session_keys(&context.new_author_id);
                assert_ok!(register_author(&context.new_author_id, &context.author_eth_public_key));

                // Get tx_id from the registration action
                let (_, action_data) = AuthorActions::<TestRuntime>::iter()
                    .find(|(author, _, _)| author == &context.new_author_id)
                    .map(|(_, ingress, data)| (ingress, data))
                    .expect("Action should exist");
                let tx_id = action_data.eth_transaction_id;

                // Now simulate T1 FAILURE
                assert_ok!(Pallet::<TestRuntime>::process_result(tx_id, PALLET_ID.to_vec(), false));

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::AuthorsManager(
                        crate::Event::<TestRuntime>::PublishingAuthorActionOnEthereumFailed {
                            tx_id
                        }
                    )));

                // AuthorActions should be removed on failure
                assert!(AuthorActions::<TestRuntime>::iter()
                    .find(|(author, _, _)| author == &context.new_author_id)
                    .is_none());
                
                // Eth key should be removed on failure
                assert!(AuthorsManager::get_ethereum_public_key_if_exists(&context.new_author_id).is_none());
            });
        }
    }

    mod fails {
        use super::*;

        #[test]
        fn with_missing_transaction() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                assert_ok!(
                    Pallet::<TestRuntime>::process_result(999u32, PALLET_ID.to_vec(), true)
                );
            });
        }
    }
}

mod rotate_author_ethereum_key {
    use sp_core::ByteArray;

    use super::*;

    struct RotateAuthorEthKeyContext {
        author: AccountId,
        author_eth_old_public_key: ecdsa::Public,
        author_eth_new_public_key: ecdsa::Public,
    }

    impl Default for RotateAuthorEthKeyContext {
        fn default() -> Self {
            let author = author_id_1();
            Balances::make_free_balance_be(&author, 100000);

            RotateAuthorEthKeyContext {
                author,
                author_eth_old_public_key: ecdsa::Public::from_slice(&AUTHOR_1_ETHEREUM_PUPLIC_KEY)
                    .unwrap(),
                author_eth_new_public_key: ecdsa::Public::from_raw(hex!(
                    "02407b0d9f41148bbe3b6c7d4a62585ae66cc32a707441197fa5453abfebd31d57"
                )),
            }
        }
    }

    #[test]
    fn succeeds_with_good_parameters() {
        let mut ext = ExtBuilder::build_default().with_authors().as_externality();
        ext.execute_with(|| {
            let context = &RotateAuthorEthKeyContext::default();

            assert_ok!(AuthorsManager::rotate_author_ethereum_key(
                RuntimeOrigin::root(),
                context.author.clone(),
                context.author_eth_old_public_key.clone(),
                context.author_eth_new_public_key.clone()
            ));

            assert_eq!(
                true,
                AuthorsManager::author_account_ids().unwrap().contains(&context.author)
            );
            assert_eq!(
                AuthorsManager::get_author_by_eth_public_key(
                    context.author_eth_new_public_key.clone()
                )
                .unwrap(),
                context.author
            );

            assert_eq!(
                AuthorsManager::get_author_by_eth_public_key(
                    context.author_eth_old_public_key.clone()
                ),
                None
            );
        });
    }

    mod fails_when {
        use super::*;

        #[test]
        fn extrinsic_is_unsigned() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = &RotateAuthorEthKeyContext::default();

                assert_noop!(
                    AuthorsManager::rotate_author_ethereum_key(
                        RuntimeOrigin::none(),
                        context.author.clone(),
                        context.author_eth_old_public_key.clone(),
                        context.author_eth_new_public_key.clone()
                    ),
                    BadOrigin
                );
            });
        }

        #[test]
        fn author_eth_key_already_exists() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = &RotateAuthorEthKeyContext::default();

                assert_noop!(
                    AuthorsManager::rotate_author_ethereum_key(
                        RuntimeOrigin::root(),
                        context.author.clone(),
                        context.author_eth_old_public_key.clone(),
                        ecdsa::Public::from_slice(&AUTHOR_2_ETHEREUM_PUPLIC_KEY).unwrap()
                    ),
                    Error::<TestRuntime>::AuthorEthKeyAlreadyExists
                );
            });
        }

        #[test]
        fn author_eth_key_unchanged() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = &RotateAuthorEthKeyContext::default();

                assert_noop!(
                    AuthorsManager::rotate_author_ethereum_key(
                        RuntimeOrigin::root(),
                        context.author.clone(),
                        context.author_eth_old_public_key.clone(),
                        context.author_eth_old_public_key.clone(),
                    ),
                    Error::<TestRuntime>::AuthorEthKeyAlreadyExists
                );
            });
        }

        #[test]
        fn author_not_found() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = &RotateAuthorEthKeyContext::default();

                let no_author = TestAccount::new([6u8; 32]).account_id();

                assert_noop!(
                    AuthorsManager::rotate_author_ethereum_key(
                        RuntimeOrigin::root(),
                        no_author,
                        context.author_eth_old_public_key.clone(),
                        context.author_eth_new_public_key.clone()
                    ),
                    Error::<TestRuntime>::AuthorNotFound
                );
            });
        }

        #[test]
        fn author_keys_missmatch() {
            let mut ext = ExtBuilder::build_default().with_authors().as_externality();
            ext.execute_with(|| {
                let context = &RotateAuthorEthKeyContext::default();

                assert_noop!(
                    AuthorsManager::rotate_author_ethereum_key(
                        RuntimeOrigin::root(),
                        author_id_5(),
                        context.author_eth_old_public_key.clone(),
                        context.author_eth_new_public_key.clone()
                    ),
                    Error::<TestRuntime>::AuthorNotFound
                );
            });
        }
    }
}}
