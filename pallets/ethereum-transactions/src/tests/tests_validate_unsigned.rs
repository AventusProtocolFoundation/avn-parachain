#![cfg(test)]

use super::mock::*;
use crate::{Call, *};
use frame_support::{assert_noop, assert_ok, unsigned::ValidateUnsigned};
use sp_runtime::{testing::UintAuthorityId, transaction_validity::TransactionValidityError};

type Signature = <AuthorityId as RuntimeAppPublic>::Signature;
struct AccountsContext {
    pub validator: AccountId,
    pub other_validator: AccountId,
    pub non_validator: AccountId,
}

impl Default for AccountsContext {
    fn default() -> Self {
        let validators: Vec<u64> = EthereumTransactions::get_validator_account_ids();
        assert_eq!(validators.len(), 3);
        let non_validator = validators[validators.len() - 1] + 1;
        if validators.iter().find(|&&x| x == non_validator) != None {
            assert!(false);
        }
        AccountsContext { validator: validators[0], other_validator: validators[1], non_validator }
    }
}

const OTHER_CONTEXT: &'static [u8] = b"other_tx_context";

mod input_is_set_eth_tx_hash_for_dispatched_tx {
    use super::*;

    struct Context {
        pub accounts: AccountsContext,
        pub tx_id: TransactionId,
        pub eth_tx_hash: H256,
    }

    impl Default for Context {
        fn default() -> Self {
            Context { accounts: Default::default(), tx_id: 1, eth_tx_hash: H256::from([1; 32]) }
        }
    }

    impl Context {
        fn signature(&self) -> Signature {
            let signer = UintAuthorityId(self.accounts.validator);
            signer
                .sign(
                    &(
                        SET_ETH_TX_HASH_FOR_DISPATCHED_TX,
                        &self.accounts.validator,
                        &self.tx_id,
                        &self.eth_tx_hash,
                    )
                        .encode(),
                )
                .unwrap()
        }

        fn signature_with_invalid_context(&self) -> Signature {
            let signer = UintAuthorityId(self.accounts.validator);
            signer
                .sign(
                    &(OTHER_CONTEXT, &self.accounts.validator, &self.tx_id, &self.eth_tx_hash)
                        .encode(),
                )
                .unwrap()
        }

        fn expected_result(&self) -> ValidTransaction {
            ValidTransaction::with_tag_prefix("EthereumTransactions")
                .priority(TransactionPriority::max_value())
                .and_provides(vec![(
                    SET_ETH_TX_HASH_FOR_DISPATCHED_TX,
                    &self.accounts.validator,
                    &self.tx_id,
                    &self.eth_tx_hash,
                )
                    .encode()])
                .longevity(64_u64)
                .propagate(true)
                .build()
                .expect("This should always work")
        }
    }

    mod succeeds_implies_that {
        use super::*;

        #[test]
        fn result_is_valid_transaction() {
            let mut ext = ExtBuilder::build_default()
                .with_genesis_config()
                .with_validators()
                .as_externality();
            ext.execute_with(|| {
                let context: Context = Default::default();
                let transaction_call = Call::<TestRuntime>::set_eth_tx_hash_for_dispatched_tx {
                    submitter: context.accounts.validator,
                    candidate_tx_id: context.tx_id,
                    eth_tx_hash: context.eth_tx_hash,
                    signature: context.signature(),
                };

                assert_ok!(
                    EthereumTransactions::validate_unsigned(
                        TransactionSource::Local,
                        &transaction_call
                    ),
                    context.expected_result()
                );
            });
        }
    }

    mod fails_when {
        use super::*;

        #[test]
        fn submitter_is_not_a_validator() {
            let mut ext = ExtBuilder::build_default()
                .with_genesis_config()
                .with_validators()
                .as_externality();
            ext.execute_with(|| {
                let context: Context = Default::default();
                let transaction_call = Call::<TestRuntime>::set_eth_tx_hash_for_dispatched_tx {
                    submitter: context.accounts.non_validator,
                    candidate_tx_id: context.tx_id,
                    eth_tx_hash: context.eth_tx_hash,
                    signature: context.signature(),
                };
                assert_noop!(
                    EthereumTransactions::validate_unsigned(
                        TransactionSource::Local,
                        &transaction_call
                    ),
                    InvalidTransaction::Custom(SUBMITTER_IS_NOT_VALIDATOR)
                );
            });
        }

        mod signature_has_wrong {
            use super::*;

            #[test]
            fn candidate_tx_id() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_validators()
                    .as_externality();
                ext.execute_with(|| {
                    let context: Context = Default::default();
                    let other_tx_id = context.tx_id + 1;
                    let transaction_call = Call::<TestRuntime>::set_eth_tx_hash_for_dispatched_tx {
                        submitter: context.accounts.validator,
                        candidate_tx_id: other_tx_id,
                        eth_tx_hash: context.eth_tx_hash,
                        signature: context.signature(),
                    };
                    assert_noop!(
                        EthereumTransactions::validate_unsigned(
                            TransactionSource::Local,
                            &transaction_call
                        ),
                        TransactionValidityError::Invalid(InvalidTransaction::BadProof)
                    );
                });
            }

            #[test]
            fn submitter() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_validators()
                    .as_externality();
                ext.execute_with(|| {
                    let context: Context = Default::default();
                    let transaction_call = Call::<TestRuntime>::set_eth_tx_hash_for_dispatched_tx {
                        submitter: context.accounts.other_validator,
                        candidate_tx_id: context.tx_id,
                        eth_tx_hash: context.eth_tx_hash,
                        signature: context.signature(),
                    };
                    assert_noop!(
                        EthereumTransactions::validate_unsigned(
                            TransactionSource::Local,
                            &transaction_call
                        ),
                        TransactionValidityError::Invalid(InvalidTransaction::BadProof)
                    );
                });
            }

            #[test]
            fn eth_tx_hash() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_validators()
                    .as_externality();
                ext.execute_with(|| {
                    let context: Context = Default::default();
                    let other_tx_hash = H256::from([2; 32]);
                    assert_ne!(context.eth_tx_hash, other_tx_hash);
                    let transaction_call = Call::<TestRuntime>::set_eth_tx_hash_for_dispatched_tx {
                        submitter: context.accounts.validator,
                        candidate_tx_id: context.tx_id,
                        eth_tx_hash: other_tx_hash,
                        signature: context.signature(),
                    };
                    assert_noop!(
                        EthereumTransactions::validate_unsigned(
                            TransactionSource::Local,
                            &transaction_call
                        ),
                        TransactionValidityError::Invalid(InvalidTransaction::BadProof)
                    );
                });
            }

            #[test]
            fn context() {
                let mut ext = ExtBuilder::build_default()
                    .with_genesis_config()
                    .with_validators()
                    .as_externality();
                ext.execute_with(|| {
                    let context: Context = Default::default();
                    let transaction_call = Call::<TestRuntime>::set_eth_tx_hash_for_dispatched_tx {
                        submitter: context.accounts.validator,
                        candidate_tx_id: context.tx_id,
                        eth_tx_hash: context.eth_tx_hash,
                        signature: context.signature_with_invalid_context(),
                    };
                    assert_noop!(
                        EthereumTransactions::validate_unsigned(
                            TransactionSource::Local,
                            &transaction_call
                        ),
                        TransactionValidityError::Invalid(InvalidTransaction::BadProof)
                    );
                });
            }
        }
    }
}
