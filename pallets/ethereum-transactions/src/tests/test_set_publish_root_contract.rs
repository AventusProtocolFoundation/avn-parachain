// Copyright 2022 Aventus (UK) Ltd.
#![cfg(test)]

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::BadOrigin;
use system::RawOrigin;

mod test_set_publish_root_contract {
    use super::*;

    struct Context {
        origin: RuntimeOrigin,
        new_contract_address: H160,
    }

    impl Default for Context {
        fn default() -> Self {
            Context { origin: RawOrigin::Root.into(), new_contract_address: H160::from([15u8; 20]) }
        }
    }

    impl Context {
        fn dispatch_set_publish_root_contract(&self) -> DispatchResult {
            return EthereumTransactions::set_publish_root_contract(
                self.origin.clone(),
                self.new_contract_address.clone(),
            )
        }
    }

    mod successful_cases {
        use super::*;
        #[test]
        fn update_publish_root_contract() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                assert_ok!(context.dispatch_set_publish_root_contract());
                assert_eq!(
                    context.new_contract_address,
                    EthereumTransactions::get_publish_root_contract()
                );
            });
        }
    }

    mod fails_when {
        use super::*;

        #[test]
        fn zero_contract_should_fail() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context: Context =
                    Context { new_contract_address: H160::zero(), ..Default::default() };

                assert_noop!(
                    context.dispatch_set_publish_root_contract(),
                    Error::<TestRuntime>::InvalidContractAddress
                );
                assert_ne!(
                    context.new_contract_address,
                    EthereumTransactions::get_publish_root_contract()
                );
            });
        }

        #[test]
        fn origin_is_not_root() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context: Context = Context {
                    origin: RuntimeOrigin::signed(Default::default()),
                    ..Default::default()
                };

                assert_noop!(context.dispatch_set_publish_root_contract(), BadOrigin);
                assert_ne!(
                    context.new_contract_address,
                    EthereumTransactions::get_publish_root_contract()
                );
            });
        }

        #[test]
        fn origin_is_unsigned() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context: Context =
                    Context { origin: RawOrigin::None.into(), ..Default::default() };

                assert_noop!(context.dispatch_set_publish_root_contract(), BadOrigin);
                assert_ne!(
                    context.new_contract_address,
                    EthereumTransactions::get_publish_root_contract()
                );
            });
        }
    }
}
