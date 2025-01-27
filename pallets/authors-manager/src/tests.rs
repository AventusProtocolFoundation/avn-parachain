//Copyright 2024 Aventus Network Services (UK) Ltd.

#![cfg(test)]

use crate::{mock::*, AVN, *};
use frame_support::{assert_noop, assert_ok, traits::Currency};
use frame_system::RawOrigin;
use hex_literal::hex;
use sp_runtime::{testing::UintAuthorityId, traits::BadOrigin};
use substrate_test_utils::assert_eq_uvec;

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

                // Result OK
                assert_ok!(register_author(&context.new_author_id, &context.author_eth_public_key));
                // Upon completion author has been added AuthorAccountIds storage
                assert!(AuthorsManager::author_account_ids()
                    .unwrap()
                    .iter()
                    .any(|a| a == &context.new_author_id));
                // AuthorRegistered Event has been deposited
                assert_eq!(
                    true,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::AuthorsManager(
                            crate::Event::<TestRuntime>::AuthorRegistered {
                                author_id: context.new_author_id,
                                eth_key: context.author_eth_public_key.clone()
                            }
                        ))
                );
                // AuthorActivationStarted Event has not been deposited yet
                assert_eq!(
                    false,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::AuthorsManager(
                            crate::Event::<TestRuntime>::AuthorActivationStarted {
                                author_id: context.new_author_id
                            }
                        ))
                );
                // But the activation action has been triggered
                assert_eq!(
                    true,
                    find_author_activation_action(
                        &context,
                        AuthorsActionStatus::AwaitingConfirmation
                    )
                );
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

                // It takes 2 session for authors to be updated
                advance_session();
                advance_session();

                // The activation action has been sent
                assert_eq!(
                    true,
                    find_author_activation_action(&context, AuthorsActionStatus::Confirmed)
                );
                // AuthorActivationStarted Event has been deposited
                assert_eq!(
                    true,
                    System::events().iter().any(|a| a.event ==
                        mock::RuntimeEvent::AuthorsManager(
                            crate::Event::<TestRuntime>::AuthorActivationStarted {
                                author_id: context.new_author_id
                            }
                        ))
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

            //Event emitted as expected
            assert!(System::events().iter().any(|a| a.event ==
                mock::RuntimeEvent::AuthorsManager(
                    crate::Event::<TestRuntime>::AuthorDeregistered {
                        author_id: context.new_author_id
                    }
                )));

            //Author removed from authors manager
            assert_eq!(
                AuthorsManager::author_account_ids()
                    .unwrap()
                    .iter()
                    .position(|&x| x == context.new_author_id),
                None
            );

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

            assert_eq!(
                true,
                AuthorsManager::author_account_ids().unwrap().contains(&context.author)
            );
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
}
