use crate::{mock::*, OperationType};
use sp_runtime::testing::UintAuthorityId;

#[test]
fn next_validator_is_incremented_correctly_for_ethereum_operations() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        let eth_index_before: u8 = AVN::get_primary_validator().0;
        AVN::calculate_primary_validator(OperationType::Ethereum);
        let eth_index_after: u8 = AVN::get_primary_validator().0;
        assert_eq!(eth_index_after, eth_index_before + 1);
    });
}

#[test]
fn the_first_validator_is_picked_again_after_the_last_validator_for_ethereum_operations() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        let validators = AVN::validators();
        let eth_index_before: u8 = AVN::get_primary_validator().0;

        for _ in &validators {
            AVN::calculate_primary_validator(OperationType::Ethereum).unwrap();
        }

        let eth_index_after: u8 = AVN::get_primary_validator().0;
        assert_eq!(eth_index_before, eth_index_after);
    });
}

#[test]
fn is_primary_function_correctly_returns_the_current_primary_validator_for_ethereum_operations() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        let expected_primary = 0;
        let result = AVN::is_primary(OperationType::Ethereum, &expected_primary);
        assert!(result.is_ok(), "Getting primary validator failed");

        AVN::calculate_primary_validator(OperationType::Ethereum);

        let next_expected_primary = expected_primary + 1;
        let result = AVN::is_primary(OperationType::Ethereum, &next_expected_primary);
        assert!(result.is_ok(), "Getting primary validator failed");
    });
}

#[test]
fn next_validator_is_incremented_correctly_for_avn_operations() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        let avn_index_before: u8 = AVN::get_primary_validator().1;
        AVN::calculate_primary_validator(OperationType::Avn);
        let avn_index_after: u8 = AVN::get_primary_validator().1;
        assert_eq!(avn_index_after, avn_index_before + 1);
    });
}

#[test]
fn the_first_validator_is_picked_again_after_the_last_validator_for_avn_operations() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        let validators = AVN::validators();
        let eth_index_before: u8 = AVN::get_primary_validator().1;

        for _ in &validators {
            AVN::calculate_primary_validator(OperationType::Avn).unwrap();
        }

        let eth_index_after: u8 = AVN::get_primary_validator().1;
        assert_eq!(eth_index_before, eth_index_after);
    });
}

#[test]
fn is_primary_function_correctly_returns_the_current_primary_validator_for_avn_operations() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        let expected_primary = 0;
        let result = AVN::is_primary(OperationType::Avn, &expected_primary);
        assert!(result.is_ok(), "Getting primary validator failed");

        AVN::calculate_primary_validator(OperationType::Avn);

        let next_expected_primary = expected_primary + 1;
        let result = AVN::is_primary(OperationType::Avn, &next_expected_primary);
        assert!(result.is_ok(), "Getting primary validator failed");
    });
}
/*********************** */

#[test]
fn test_local_authority_keys_empty() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        let current_node_validator = AVN::get_validator_for_current_node();
        assert!(current_node_validator.is_none());
    });
}

#[test]
fn test_local_authority_keys_valid() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();
    ext.execute_with(|| {
        UintAuthorityId::set_all_keys(vec![1, 2, 3]);
        let current_node_validator = AVN::get_validator_for_current_node().unwrap();
        assert_eq!(current_node_validator.account_id, 1);
        assert_eq!(current_node_validator.key, UintAuthorityId(1));
    });
}

/**************************** */
