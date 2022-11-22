// Copyright 2022 Aventus Network Services (UK) Ltd.

use crate::{
    mock::*,
};
use sp_runtime::testing::UintAuthorityId;

// TODO [TYPE: test refactoring][PRI: LOW]:  update this function to work with the mock builder
// pattern Currently, a straightforward replacement of the test setup leads to an error on the
// assert_eq!

fn avn_known_collators() -> sp_application_crypto::Vec<sp_avn_common::event_types::Validator<AuthorityId, sp_core::sr25519::Public>> {
    return AVN::validators();
}

fn add_collator_candidate(id: AccountId, auth_id: u64) {
    let new_candidate_id = id;
    let auth_id = UintAuthorityId(auth_id);
    add_collator(&new_candidate_id, auth_id);
}

fn remove_collator_candidate(id: AccountId, validators_count: u32) {
    let new_candidate_id = id;
    remove_collator(&new_candidate_id, validators_count);
}

mod chain_started_with_initial_colators {
    use super::*;

    fn setup_initial_collators() -> sp_io::TestExternalities {
        let initial_validators_staking: Vec<(sp_core::sr25519::Public, u128)> = vec![
            (TestAccount::derive_account_id(1), 100),
            (TestAccount::derive_account_id(2), 100),
            (TestAccount::derive_account_id(3), 100),
            ];

        let initial_validators_session:Vec<u64> = vec![1,2,3];

        let initial_account_balances: Vec<(AccountId, Balance)> = vec![
            (TestAccount::derive_account_id(1), 10000),
            (TestAccount::derive_account_id(2), 10000),
            (TestAccount::derive_account_id(3), 10000),
            (TestAccount::derive_account_id(4), 10000),
            (TestAccount::derive_account_id(5), 10000),
            (TestAccount::derive_account_id(6), 10000),
            (TestAccount::derive_account_id(7), 10000),
            (TestAccount::derive_account_id(8), 10000),
            (TestAccount::derive_account_id(9), 10000),
            (TestAccount::derive_account_id(10), 10000),
            ];

        let mut ext = ExtBuilder::build_default().with_balances(initial_account_balances.clone()).with_validators(initial_validators_session).with_staking(initial_validators_staking).as_externality();
        ext
    }

    #[test]
    fn all_and_only_initial_collators_are_registered_with_avn_pallet_at_startup() {
        let mut ext = setup_initial_collators();

        ext.execute_with(|| {
            assert!(
                AVN::validators() ==
                vec![ TestAccount::derive_validator(1), TestAccount::derive_validator(2), TestAccount::derive_validator(3)]
            );
        });
    }

    #[test]
    fn if_no_changes_between_sessions_then_avn_knows_same_collators()
    {
        let mut ext = setup_initial_collators();

        ext.execute_with(|| {
            let initial_collators = avn_known_collators();

            advance_session();

            let current_collators = avn_known_collators();

            advance_session();

            assert_eq!(initial_collators, current_collators);
            assert_eq!(current_collators, vec![
                TestAccount::derive_validator(1),
                TestAccount::derive_validator(2),
                TestAccount::derive_validator(3),
            ]);
        });
    }

    mod when_new_candidate_registers {
        use super::*;

    #[test]
        fn then_no_change_visible_in_following_session(){
            let mut ext = setup_initial_collators();
            let added_valditator = TestAccount::derive_validator(4);

            ext.execute_with(|| {
                let initial_collators = avn_known_collators();
                add_collator_candidate(added_valditator.account_id, 4);

                advance_session();

                let final_collators = avn_known_collators();
                assert_eq!(initial_collators, final_collators);
            })
        }

    #[test]
        fn then_avn_knows_collator_after_two_sessions(){
            let mut ext = setup_initial_collators();
            let added_valditator = TestAccount::derive_validator(4);

            ext.execute_with(|| {
                add_collator_candidate(added_valditator.account_id, 4);
                advance_session();
                advance_session();

                let final_collators = avn_known_collators();

                assert_eq!(final_collators,
                    vec![
                    TestAccount::derive_validator(3),
                    TestAccount::derive_validator(4),
                    TestAccount::derive_validator(1),
                    TestAccount::derive_validator(2),
                    ]);
            })
        }

    #[test]
        fn with_new_key_then_avn_information_is_updated(){
            let mut ext = setup_initial_collators();
            ext.execute_with(|| {
                let added_valditator = TestAccount::derive_validator(3);
                add_collator_candidate(added_valditator.account_id, 4);

                advance_session();
                advance_session();

                let final_collators = avn_known_collators();
                assert_eq!(final_collators, vec![
                    TestAccount::derive_validator_key(3, 4),
                    TestAccount::derive_validator(1),
                    TestAccount::derive_validator(2),
                ]);
            })
        }
    }

    mod when_collator_removed {
        use super::*;

        fn add_two_collators_and_force_two_sessions() {
            add_collator_candidate(TestAccount::derive_validator(4).account_id, 4);
            add_collator_candidate(TestAccount::derive_validator(5).account_id, 5);

            advance_session();
            advance_session();
        }

        #[test]
        fn then_no_change_visible_in_following_session(){
            let mut ext = setup_initial_collators();

            ext.execute_with(|| {
                add_two_collators_and_force_two_sessions();

                let current_collators = avn_known_collators();
                assert_eq!(current_collators, vec![
                    TestAccount::derive_validator(5),
                    TestAccount::derive_validator(3),
                    TestAccount::derive_validator(4),
                    TestAccount::derive_validator(1),
                    TestAccount::derive_validator(2),
                ]);

                remove_collator_candidate(TestAccount::derive_validator(5).account_id, 5);

                advance_session();

                let final_collators = avn_known_collators();
                assert_eq!(final_collators, vec![
                    TestAccount::derive_validator(5),
                    TestAccount::derive_validator(3),
                    TestAccount::derive_validator(4),
                    TestAccount::derive_validator(1),
                    TestAccount::derive_validator(2),
                ]);
            })
        }

        #[test]
        fn then_avn_does_not_know_collator(){
            let mut ext = setup_initial_collators();

            ext.execute_with(|| {
                add_two_collators_and_force_two_sessions();

                let current_collators = avn_known_collators();
                assert_eq!(current_collators, vec![
                    TestAccount::derive_validator(5),
                    TestAccount::derive_validator(3),
                    TestAccount::derive_validator(4),
                    TestAccount::derive_validator(1),
                    TestAccount::derive_validator(2),
                ]);

                remove_collator_candidate(TestAccount::derive_validator(5).account_id, 5);

                advance_session();
                advance_session();

                let final_collators = avn_known_collators();
                assert_eq!(final_collators, vec![
                    TestAccount::derive_validator(3),
                    TestAccount::derive_validator(4),
                    TestAccount::derive_validator(1),
                    TestAccount::derive_validator(2),
                ]);
            })
        }
    }

    mod when_more_than_desired_candidates_register {
        use super::*;

        fn setup_adds_seven_collators() {
            for id in 4u64..10u64 {
                super::add_collator_candidate(TestAccount::derive_validator(id).account_id, id);
            }
        }

        #[test]
        fn then_no_change_visible_in_following_session(){
            let mut ext = setup_initial_collators();
            ext.execute_with(|| {
                let initial_collators = avn_known_collators();
                setup_adds_seven_collators();

                advance_session();

                let final_collators = avn_known_collators();

                assert_eq!(initial_collators, final_collators);
            })
        }

        #[test]
        fn then_after_two_sessions_avn_knows_subset_of_new_candidates(){
            let mut ext = setup_initial_collators();
            ext.execute_with(|| {
                setup_adds_seven_collators();

                advance_session();
                advance_session();

                let final_collators = avn_known_collators();

                assert_eq!(final_collators, vec![
                    TestAccount::derive_validator(5),
                    TestAccount::derive_validator(3),
                    TestAccount::derive_validator(4),
                    TestAccount::derive_validator(1),
                    TestAccount::derive_validator(2),
                ]);
            })
        }
    }

}