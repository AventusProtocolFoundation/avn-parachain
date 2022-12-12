use crate::mock::*;
use sp_runtime::testing::UintAuthorityId;

#[test]
fn test_is_primary_blocknumber_1() {
    let mut ext = ExtBuilder::build_default().with_validators(vec![1, 2, 3]).as_externality();
    ext.execute_with(|| {
        let block_number = 1;
        let expected_primary = TestAccount::derive_validator(2).account_id;
        let result = AVN::is_primary(block_number, &expected_primary);
        assert!(result.is_ok(), "Getting primary validator failed");
        assert_eq!(result.unwrap(), true);
    });
}

#[test]
fn test_is_primary_blocknumber_2() {
    let mut ext = ExtBuilder::build_default().with_validators(vec![1, 2, 3]).as_externality();
    ext.execute_with(|| {
        let block_number = 2;
        let expected_primary = TestAccount::derive_validator(3).account_id;
        let result = AVN::is_primary(block_number, &expected_primary);
        assert!(result.is_ok(), "Getting primary validator failed");
        assert_eq!(result.unwrap(), true);
    });
}

#[test]
fn test_is_primary_blocknumber_3() {
    let mut ext = ExtBuilder::build_default().with_validators(vec![1, 2, 3]).as_externality();
    ext.execute_with(|| {
        let block_number = 3;
        let expected_primary = TestAccount::derive_validator(1).account_id;
        let result = AVN::is_primary(block_number, &expected_primary);
        assert!(result.is_ok(), "Getting primary validator failed");
        assert_eq!(result.unwrap(), true);
    });
}

#[test]
fn test_is_primary_blocknumber_100() {
    let mut ext = ExtBuilder::build_default().with_validators(vec![1, 2, 3]).as_externality();
    ext.execute_with(|| {
        let block_number = 100;
        let expected_primary = TestAccount::derive_validator(2).account_id;
        let result = AVN::is_primary(block_number, &expected_primary);
        assert!(result.is_ok(), "Getting primary validator failed");
        assert_eq!(result.unwrap(), true);
    });
}

#[test]
fn is_primary_fails_with_no_validators() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let block_number = 1;
        let dummy_validator = TestAccount::derive_validator(1).account_id;
        let result = AVN::is_primary(block_number, &dummy_validator);
        assert!(result.is_err(), "Getting primary validator should have failed");
    });
}

/*********************** */

#[test]
fn test_local_authority_keys_empty() {
    let mut ext = ExtBuilder::build_default().with_validators(vec![1, 2, 3]).as_externality();
    ext.execute_with(|| {
        let current_node_validator = AVN::get_validator_for_current_node();
        assert!(current_node_validator.is_none());
    });
}

#[test]
fn test_local_authority_keys_valid() {
    let mut ext = ExtBuilder::build_default().with_validators(vec![1, 2, 3]).as_externality();
    ext.execute_with(|| {
        UintAuthorityId::set_all_keys(vec![1, 2, 3]);
        let current_node_validator = AVN::get_validator_for_current_node().unwrap();
        assert_eq!(current_node_validator.account_id, TestAccount::derive_account_id(1));
        assert_eq!(current_node_validator.key, UintAuthorityId(1));
    });
}

/**************************** */
