#![cfg(test)]

use crate::ethereum_transaction::*;
use ethabi::{Function, Param, ParamType, Token};
use sp_core::H512;

pub const ROOT_HASH: [u8; 32] = [3; 32];
pub const T2_PUBLIC_KEY: [u8; 32] = [4; 32];
pub const T1_PUBLIC_KEY: [u8; 64] = [5u8; 64];
pub const GROWTH_PERIOD: u32 = 1;
pub const REWARDS_IN_PERIOD: u128 = 100;
pub const AVERAGE_STAKED_IN_PERIOD: u128 = 400;

pub fn generate_publish_root_data(root_hash: [u8; 32]) -> PublishRootData {
    PublishRootData { root_hash }
}

fn generate_deregister_validator_data(
    t1_public_key: H512,
    t2_public_key: [u8; 32],
) -> DeregisterCollatorData {
    DeregisterCollatorData { t1_public_key, t2_public_key }
}

fn generate_trigger_growth_data(
    rewards_in_period: u128,
    average_staked_in_period: u128,
    period: u32,
) -> TriggerGrowthData {
    TriggerGrowthData { rewards_in_period, average_staked_in_period, period }
}

fn generate_publish_root_eth_txn_desc(root_hash: [u8; 32]) -> EthTransactionDescription {
    EthTransactionDescription {
        function_call: Function {
            name: String::from("publishRoot"),
            inputs: vec![Param {
                name: String::from("_rootHash"),
                kind: ParamType::FixedBytes(32),
            }],
            outputs: Vec::<Param>::new(),
            constant: false,
        },
        call_values: vec![Token::FixedBytes(root_hash.to_vec())],
    }
}
fn generate_deregister_collator_eth_txn_desc(
    t1_public_key: H512,
    t2_public_key: [u8; 32],
) -> EthTransactionDescription {
    EthTransactionDescription {
        function_call: Function {
            name: String::from("deregisterValidator"),
            inputs: vec![
                Param { name: String::from("_targetT1PublicKey"), kind: ParamType::Bytes },
                Param { name: String::from("_targetT2PublicKey"), kind: ParamType::FixedBytes(32) },
            ],
            outputs: Vec::<Param>::new(),
            constant: false,
        },
        call_values: vec![
            Token::Bytes(t1_public_key.to_fixed_bytes().to_vec()),
            Token::FixedBytes(t2_public_key.to_vec()),
        ],
    }
}

fn generate_trigger_growth_eth_txn_desc(
    rewards_in_period: u128,
    average_staked_in_period: u128,
    period: u32,
) -> EthTransactionDescription {
    EthTransactionDescription {
        function_call: Function {
            name: String::from("triggerGrowth"),
            inputs: vec![
                Param { name: String::from("rewards_in_period"), kind: ParamType::Uint(128) },
                Param {
                    name: String::from("average_staked_in_period"),
                    kind: ParamType::Uint(128),
                },
                Param { name: String::from("period"), kind: ParamType::Uint(32) },
            ],
            outputs: Vec::<Param>::new(),
            constant: false,
        },
        call_values: vec![
            Token::Uint(rewards_in_period.into()),
            Token::Uint(average_staked_in_period.into()),
            Token::Uint(period.into()),
        ],
    }
}

// EthTransactionType tests
mod eth_transaction_type {
    use super::*;

    fn generate_publish_root_eth_txn_type(root_hash: [u8; 32]) -> EthTransactionType {
        EthTransactionType::PublishRoot(generate_publish_root_data(root_hash))
    }

    fn generate_deregister_collator_eth_txn_type(
        t1_public_key: H512,
        t2_public_key: [u8; 32],
    ) -> EthTransactionType {
        EthTransactionType::DeregisterCollator(generate_deregister_validator_data(
            t1_public_key,
            t2_public_key,
        ))
    }

    fn generate_trigger_growth_eth_txn_type(
        rewards_in_period: u128,
        average_staked_in_period: u128,
        period: u32,
    ) -> EthTransactionType {
        EthTransactionType::TriggerGrowth(generate_trigger_growth_data(
            rewards_in_period,
            average_staked_in_period,
            period,
        ))
    }

    fn generate_unsupported_eth_txn_type() -> EthTransactionType {
        EthTransactionType::Invalid
    }

    mod to_abi {
        use super::*;

        mod succeeds_when {
            use super::*;

            #[test]
            fn txn_is_publish_root() {
                let publish_root_eth_txn_type = generate_publish_root_eth_txn_type(ROOT_HASH);
                let publish_root_eth_txn_desc = generate_publish_root_eth_txn_desc(ROOT_HASH);

                let result = publish_root_eth_txn_type.to_abi();

                assert!(result.is_ok(), "Unsupported ethereum transaction type!");
                assert_eq!(result.unwrap(), publish_root_eth_txn_desc);
            }

            #[test]
            fn txn_is_deregister_validator() {
                let deregister_validator_eth_txn_type = generate_deregister_collator_eth_txn_type(
                    H512::from(T1_PUBLIC_KEY),
                    T2_PUBLIC_KEY,
                );
                let deregister_validator_eth_txn_desc = generate_deregister_collator_eth_txn_desc(
                    H512::from(T1_PUBLIC_KEY),
                    T2_PUBLIC_KEY,
                );

                let result = deregister_validator_eth_txn_type.to_abi();

                assert!(result.is_ok(), "Unsupported ethereum transaction type!");
                assert_eq!(result.unwrap(), deregister_validator_eth_txn_desc);
            }

            #[test]
            fn txn_is_trigger_growth() {
                let trigger_growth_eth_txn_type = generate_trigger_growth_eth_txn_type(
                    REWARDS_IN_PERIOD,
                    AVERAGE_STAKED_IN_PERIOD,
                    GROWTH_PERIOD,
                );
                let trigger_growth_eth_txn_desc = generate_trigger_growth_eth_txn_desc(
                    REWARDS_IN_PERIOD,
                    AVERAGE_STAKED_IN_PERIOD,
                    GROWTH_PERIOD,
                );

                let result = trigger_growth_eth_txn_type.to_abi();

                assert!(result.is_ok(), "Unsupported ethereum transaction type!");
                assert_eq!(result.unwrap(), trigger_growth_eth_txn_desc);
            }
        }

        #[test]
        fn fails_when_txn_is_invalid() {
            let unsupported_eth_txn_type = generate_unsupported_eth_txn_type();

            assert!(
                unsupported_eth_txn_type.to_abi().is_err(),
                "Ethererum transaction type is valid"
            );
        }
    }
}

// PublishRootData tests
mod publish_root_data {
    use super::*;

    #[test]
    fn new_succeeds() {
        let expected_publish_root_data = generate_publish_root_data(ROOT_HASH);

        assert_eq!(PublishRootData::new(ROOT_HASH), expected_publish_root_data);
    }

    #[test]
    fn to_abi_succeeds() {
        let publish_root_data = generate_publish_root_data(ROOT_HASH);
        let expected_eth_transaction_desc = generate_publish_root_eth_txn_desc(ROOT_HASH);

        assert_eq!(publish_root_data.to_abi(), expected_eth_transaction_desc);
    }
}

// DeregisterValidatorData tests
mod deregister_validator_data {
    use super::*;

    #[test]
    fn new_succeeds() {
        let expected_deregister_validator_data =
            generate_deregister_validator_data(H512::from(T1_PUBLIC_KEY), T2_PUBLIC_KEY);

        assert_eq!(
            DeregisterCollatorData::new(H512::from(T1_PUBLIC_KEY), T2_PUBLIC_KEY),
            expected_deregister_validator_data
        );
    }

    #[test]
    fn to_abi_succeeds() {
        let deregister_validator_data =
            generate_deregister_validator_data(H512::from(T1_PUBLIC_KEY), T2_PUBLIC_KEY);
        let expected_eth_transaction_desc: EthTransactionDescription =
            generate_deregister_collator_eth_txn_desc(H512::from(T1_PUBLIC_KEY), T2_PUBLIC_KEY);

        assert_eq!(deregister_validator_data.to_abi(), expected_eth_transaction_desc);
    }
}

// TriggerGrowthData tests
mod trigger_growth_data {
    use super::*;

    #[test]
    fn new_succeeds() {
        let expected_trigger_growth_data = generate_trigger_growth_data(
            REWARDS_IN_PERIOD,
            AVERAGE_STAKED_IN_PERIOD,
            GROWTH_PERIOD,
        );

        assert_eq!(
            TriggerGrowthData::new(REWARDS_IN_PERIOD, AVERAGE_STAKED_IN_PERIOD, GROWTH_PERIOD),
            expected_trigger_growth_data
        );
    }

    #[test]
    fn to_abi_succeeds() {
        let trigger_growth_data = generate_trigger_growth_data(
            REWARDS_IN_PERIOD,
            AVERAGE_STAKED_IN_PERIOD,
            GROWTH_PERIOD,
        );
        let expected_eth_transaction_desc: EthTransactionDescription =
            generate_trigger_growth_eth_txn_desc(
                REWARDS_IN_PERIOD,
                AVERAGE_STAKED_IN_PERIOD,
                GROWTH_PERIOD,
            );

        assert_eq!(trigger_growth_data.to_abi(), expected_eth_transaction_desc);
    }
}
