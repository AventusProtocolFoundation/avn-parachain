use crate::{
    mock::{TestRuntime, *},
    Error, OperationType,
};
use frame_support::{
    assert_err,
    assert_noop,
    // dispatch::{DispatchError, DispatchResult},
};
use sp_runtime::testing::UintAuthorityId;

fn get_index_based_on_operation_type(operationType: &OperationType) -> u8 {
    match operationType {
        OperationType::Ethereum => AVN::get_primary_collator().ethereum,
        OperationType::Avn => AVN::get_primary_collator().avn,
    }
}

fn setup_last_validator_as_primary(operationType: &OperationType) {
    let validators = AVN::validators();
    let eth_index_before: u8 = get_index_based_on_operation_type(operationType);

    let num_validators_indexed = validators.len() - 1;
    for _ in &validators[..num_validators_indexed] {
        AVN::calculate_primary_validator(operationType.clone()).unwrap();
    }

    let eth_index_after: u8 = get_index_based_on_operation_type(operationType);
    assert_eq!(eth_index_before + num_validators_indexed as u8, eth_index_after);
}

#[cfg(test)]
mod when_calculate_primary_validator_is_called_for_operation_ethereum {
    use super::*;
    #[test]
    fn the_next_validator_for_ethereum_increments_by_one_if_the_current_one_is_not_the_last() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let eth_index_before: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);

            AVN::calculate_primary_validator(OperationType::Ethereum);

            let eth_index_after: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            assert_eq!(eth_index_after, eth_index_before + 1);
        });
    }

    #[test]
    fn the_validator_for_ethereum_wraps_around_if_the_current_one_is_the_last() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let eth_index_before: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            assert_eq!(eth_index_before, 0);

            setup_last_validator_as_primary(&OperationType::Ethereum);

            AVN::calculate_primary_validator(OperationType::Ethereum).unwrap();
            let eth_index_after: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            assert_eq!(eth_index_before, eth_index_after);
        });
    }

    #[test]
    fn the_validator_for_avn_stays_the_same() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let validators = AVN::validators();
            let eth_index_before: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            let avn_index_before: u8 = get_index_based_on_operation_type(&OperationType::Avn);

            AVN::calculate_primary_validator(OperationType::Ethereum).unwrap();

            let eth_index_after: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            let avn_index_after: u8 = get_index_based_on_operation_type(&OperationType::Avn);
            assert_eq!(eth_index_before + 1, eth_index_after);
            assert_eq!(avn_index_before, avn_index_after);
        });
    }
}

#[cfg(test)]
mod when_calculate_primary_validator_is_called_for_operation_avn {
    use super::*;
    #[test]
    fn the_next_validator_for_avn_increments_by_one_if_the_current_one_is_not_the_last() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let avn_index_before: u8 = get_index_based_on_operation_type(&OperationType::Avn);

            AVN::calculate_primary_validator(OperationType::Avn);

            let avn_index_after: u8 = get_index_based_on_operation_type(&OperationType::Avn);
            assert_eq!(avn_index_after, avn_index_before + 1);
        });
    }

    #[test]
    fn the_validator_for_avn_wraps_around_if_the_current_one_is_the_last() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let avn_index_before: u8 = get_index_based_on_operation_type(&OperationType::Avn);
            assert_eq!(avn_index_before, 0);

            setup_last_validator_as_primary(&OperationType::Avn);

            AVN::calculate_primary_validator(OperationType::Avn).unwrap();
            let avn_index_after: u8 = get_index_based_on_operation_type(&OperationType::Avn);
            assert_eq!(avn_index_before, avn_index_after);
        });
    }

    #[test]
    fn the_validator_for_ethereum_stays_the_same() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let validators = AVN::validators();
            let eth_index_before: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            let avn_index_before: u8 = get_index_based_on_operation_type(&OperationType::Avn);

            AVN::calculate_primary_validator(OperationType::Avn).unwrap();

            let eth_index_after: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            let avn_index_after: u8 = get_index_based_on_operation_type(&OperationType::Avn);
            assert_eq!(eth_index_before, eth_index_after);
            assert_eq!(avn_index_before + 1, avn_index_after);
        });
    }
}

#[cfg(test)]
mod calling_is_primary_validator_for_ethereum {
    use super::*;
    #[test]
    fn does_not_change_the_next_primary_validator_for_ethereum() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let expected_primary = 1;
            let eth_index_before: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            assert_eq!(eth_index_before, 0);

            let result = AVN::is_primary(OperationType::Ethereum, &expected_primary).unwrap();
            assert!(result == true, "Wrong primary validator");

            let eth_index_after: u8 = get_index_based_on_operation_type(&OperationType::Ethereum);
            assert_eq!(eth_index_before, eth_index_after);
        });
    }

    #[test]
    fn returns_true_if_the_argument_is_the_same_as_the_next_ethereum_validator() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let mut expected_primary = 1;
            let mut result = AVN::is_primary(OperationType::Ethereum, &expected_primary).unwrap();
            assert!(result == true, "Wrong primary validator");

            AVN::calculate_primary_validator(OperationType::Ethereum);

            expected_primary = 2;
            result = AVN::is_primary(OperationType::Ethereum, &expected_primary).unwrap();
            assert!(result == true, "Wrong primary validator");
        });
    }

    #[test]
    fn returns_false_if_it_is_not() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let non_primary_validator = 4;
            let result = AVN::is_primary(OperationType::Ethereum, &non_primary_validator).unwrap();
            assert!(result != true, "Primary validator is unexpectedly correct");
        });
    }

    #[test]
    fn fails_with_no_validators() {
        let mut ext = ExtBuilder::build_default().as_externality();
        ext.execute_with(|| {
            let mut expected_primary = 1;
            let result = AVN::is_primary(OperationType::Ethereum, &expected_primary);
            assert!(result.is_err());
        });
    }
}

#[cfg(test)]
mod calling_is_primary_validator_for_avn {
    use super::*;
    #[test]
    fn does_not_change_the_next_primary_validator_for_avn() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let expected_primary = 1;
            let avn_index_before: u8 = get_index_based_on_operation_type(&OperationType::Avn);
            assert_eq!(avn_index_before, 0);

            let result = AVN::is_primary(OperationType::Avn, &expected_primary).unwrap();
            assert!(result == true, "Wrong primary validator");

            let avn_index_after: u8 = get_index_based_on_operation_type(&OperationType::Avn);
            assert_eq!(avn_index_before, avn_index_after);
        });
    }

    #[test]
    fn returns_true_if_the_argument_is_the_same_as_the_next_avn_validator() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let mut expected_primary = 1;
            let mut result = AVN::is_primary(OperationType::Avn, &expected_primary).unwrap();
            assert!(result == true, "Wrong primary validator");

            AVN::calculate_primary_validator(OperationType::Avn);

            let mut expected_primary = 2;
            let mut result = AVN::is_primary(OperationType::Avn, &expected_primary).unwrap();
            assert!(result == true, "Wrong primary validator");
        });
    }

    #[test]
    fn returns_false_if_it_is_not() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let mut expected_primary = 4;
            let mut result = AVN::is_primary(OperationType::Avn, &expected_primary).unwrap();
            assert!(result != true, "Primary validator is unexpectedly correct");
        });
    }

    #[test]
    fn fails_with_no_validators() {
        let mut ext = ExtBuilder::build_default().as_externality();
        ext.execute_with(|| {
            let mut expected_primary = 1;
            let result = AVN::is_primary(OperationType::Avn, &expected_primary);
            assert!(result.is_err());
        });
    }
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
